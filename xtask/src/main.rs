//! Build automation tasks for safe-pdf.
//!
//! Usage:
//!   cargo xtask emscripten [--profile release|debug] [--features skia-wasm] [--serve] [--port 8080] [--emsdk <path>]
//!
//! This crate provides idiomatic Rust tooling for building and packaging
//! the emscripten WebAssembly example.

use anyhow::{Context, Result, bail};
mod annotation_types;
use clap::{Parser, Subcommand, ValueEnum};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    fn as_target_dir_name(self) -> &'static str {
        match self {
            BuildProfile::Debug => "debug",
            BuildProfile::Release => "release",
        }
    }

    fn is_release(self) -> bool {
        matches!(self, BuildProfile::Release)
    }
}

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Build automation tasks for safe-pdf", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate browser annotation types from the Serde presentation contract.
    AnnotationTypes {
        /// Fail if the checked-in contract differs from the Rust types.
        #[arg(long)]
        check: bool,
    },
    /// Build the Canvas 2D wasm-bindgen example.
    WebCanvas {
        /// Build profile.
        #[arg(long, value_enum, default_value_t = BuildProfile::Release)]
        profile: BuildProfile,
        /// Serve the standalone example after building.
        #[arg(long)]
        serve: bool,
        /// Local HTTP port.
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Build the emscripten WebAssembly example
    Emscripten {
        /// Cargo build profile
        #[arg(long, value_enum, default_value_t = BuildProfile::Release)]
        profile: BuildProfile,

        /// Cargo features to enable
        #[arg(long, default_value = "skia-wasm")]
        features: String,

        /// Path to Emscripten SDK (defaults to $EMSDK, then ~/emsdk)
        #[arg(long, env = "EMSDK")]
        emsdk: Option<PathBuf>,

        /// Rust toolchain for this target; rust-skia 0.91's prebuilt Emscripten
        /// libraries need the JavaScript exception ABI that Rust 1.93+ dropped.
        #[arg(long, default_value = EMSCRIPTEN_TOOLCHAIN)]
        toolchain: String,

        /// Start a local dev server after building
        #[arg(long)]
        serve: bool,

        /// Port for the dev server
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Clean build artifacts
    Clean,
    /// Generate compile-time predefined CMap tables from Adobe CMap resources
    GenerateCmaps {
        /// Directory containing an Adobe cmap-resources checkout
        #[arg(long)]
        source_dir: PathBuf,

        /// Output Rust file
        #[arg(long, default_value = "crates/pdf-cmap/src/predefined/generated.rs")]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::AnnotationTypes { check } => annotation_types::generate(check)?,
        Commands::WebCanvas {
            profile,
            serve,
            port,
        } => {
            build_web_canvas(profile)?;
            if serve {
                let root = project_root()?.join("examples/web-canvas");
                let status = Command::new("python3")
                    .args([
                        "-m",
                        "http.server",
                        &port.to_string(),
                        "--bind",
                        "127.0.0.1",
                    ])
                    .current_dir(root)
                    .status()
                    .context("starting web server")?;
                if !status.success() {
                    bail!("web server exited with {status}");
                }
            }
        }
        Commands::Emscripten {
            profile,
            features,
            emsdk,
            toolchain,
            serve,
            port,
        } => {
            build_emscripten(profile, &features, emsdk.as_deref(), &toolchain)?;
            if serve {
                serve_examples(port)?;
            }
        }
        Commands::Clean => {
            clean()?;
        }
        Commands::GenerateCmaps { source_dir, output } => {
            generate_cmaps(&source_dir, &output)?;
        }
    }

    Ok(())
}

/// Returns the workspace root directory.
fn project_root() -> Result<PathBuf> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;

    let root = Path::new(&manifest_dir)
        .parent()
        .context("Failed to get parent directory")?
        .to_path_buf();

    Ok(root)
}

/// Newest Rust whose `wasm32-unknown-emscripten` std still links with the
/// JavaScript exception and longjmp ABI used by rust-skia 0.91's binaries.
const EMSCRIPTEN_TOOLCHAIN: &str = "1.92.0";

/// Builds the emscripten example and copies artifacts to `examples/web/dist/`.
fn build_emscripten(
    profile: BuildProfile,
    features: &str,
    emsdk_override: Option<&Path>,
    toolchain: &str,
) -> Result<()> {
    let root = project_root()?;
    let dist_dir = root.join("examples").join("web").join("dist");

    println!("🔧 Building emscripten example...");

    // Check emsdk exists
    let emsdk_path = get_emsdk_path(emsdk_override)?;

    // Set EMCC_CFLAGS
    // Keep setjmp/longjmp on the JavaScript ABI used by rust-skia's
    // precompiled Emscripten libraries.
    let emcc_cflags = [
        "--no-entry",
        "-sASSERTIONS=1",
        "-sALLOW_TABLE_GROWTH=1",
        "-sALLOW_MEMORY_GROWTH=1",
        "-sENVIRONMENT=web",
        "-sERROR_ON_UNDEFINED_SYMBOLS=0",
        "-sMAX_WEBGL_VERSION=2",
        "-sSUPPORT_LONGJMP=emscripten",
    ]
    .join(" ");

    // Set RUSTFLAGS for exported functions and memory configuration.
    //
    // Memory flags are set here (via -C link-args) to guarantee they reach
    // the final emcc link step. Relying solely on EMCC_CFLAGS is fragile
    // because cargo may invoke emcc for linking without that env var.
    //
    // INITIAL_MEMORY  – 128 MiB.  Skia's static data segments + Rust
    //   runtime consume most of the default 16 MiB, leaving almost no heap.
    // STACK_SIZE      – 2 MiB.  The default 64 KiB is too small for Rust.
    // ALLOW_MEMORY_GROWTH – duplicated from EMCC_CFLAGS to ensure it
    //   reaches the linker; the heap can grow beyond INITIAL_MEMORY.
    let rustflags = [
        "-C link-args=-sEXPORTED_FUNCTIONS=['_sk_load_pdf','_sk_get_prefetch_count','_sk_get_prefetch_page','_sk_get_page_count','_sk_render_page','_sk_free_pdf','_sk_reset_gpu','_sk_is_page_cached','_sk_get_cache_count','_sk_clear_cache','_sk_get_page_width','_sk_get_page_height','_sk_get_scratch_ptr','_sk_hit_test_text','_sk_select','_sk_build_selection_updates','_sk_build_selected_text','_malloc','_free','_main']",
        "-C link-args=-sEXPORTED_RUNTIME_METHODS=['cwrap','HEAPU8','HEAPU32','GL']",
        // An ES module factory: no global `Module`, and `GL` reachable on the instance.
        "-C link-args=-sMODULARIZE=1",
        "-C link-args=-sEXPORT_ES6=1",
        "-C link-args=-sEXPORT_NAME=createSafePdfModule",
        "-C link-args=-sSTANDALONE_WASM=0",
        "-C link-args=-sINITIAL_MEMORY=134217728",
        "-C link-args=-sSTACK_SIZE=2097152",
        "-C link-args=-sALLOW_MEMORY_GROWTH=1",
        "-C link-args=-sSUPPORT_LONGJMP=emscripten",
    ]
    .join(" ");

    // Build cargo command
    let mut cargo_args = vec![
        "build".to_string(),
        "-p".to_string(),
        "examples".to_string(),
        "--bin".to_string(),
        "emscripten".to_string(),
        "--features".to_string(),
        features.to_string(),
        "--target".to_string(),
        "wasm32-unknown-emscripten".to_string(),
    ];

    if profile.is_release() {
        cargo_args.push("--release".to_string());
        // The Emscripten std of the pinned toolchain requires unwinding; override
        // the workspace's `panic = "abort"` release setting for this target only.
        cargo_args.push("--config".to_string());
        cargo_args.push("profile.release.panic=\"unwind\"".to_string());
    }

    println!(
        "📦 Running: cargo {} (toolchain {toolchain})",
        cargo_args.join(" ")
    );

    // Run cargo build through bash with emsdk environment sourced.
    // Use `exec "$@"` to avoid shell interpolation/injection issues.
    let emsdk_env = emsdk_path.join("emsdk_env.sh");
    let bash_script = format!(
        "set -euo pipefail\nsource {} >/dev/null 2>&1\ncd {}\nexec \"$@\"\n",
        bash_single_quote(emsdk_env.to_string_lossy().as_ref()),
        bash_single_quote(root.to_string_lossy().as_ref()),
    );

    let status = Command::new("bash")
        .arg("-c")
        .arg(&bash_script)
        .arg("bash")
        .arg("cargo")
        .args(&cargo_args)
        .env("EMCC_CFLAGS", emcc_cflags)
        .env("RUSTFLAGS", rustflags)
        .env("RUSTUP_TOOLCHAIN", toolchain)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to execute cargo build")?;

    if !status.success() {
        bail!("Cargo build failed with status: {status}");
    }

    // Copy artifacts
    let target_dir = root
        .join("target")
        .join("wasm32-unknown-emscripten")
        .join(profile.as_target_dir_name());

    copy_artifacts(&target_dir, &dist_dir)?;

    println!("✅ Build complete! Artifacts copied to examples/web/dist/");
    println!();
    println!("To serve locally, run:");
    println!("  cargo xtask emscripten --serve --port {}", 8080);
    println!("  # or");
    println!("  cd examples/web && python3 -m http.server {}", 8080);

    Ok(())
}

/// Returns the path to the Emscripten SDK, or an error if not found.
fn get_emsdk_path(emsdk_override: Option<&Path>) -> Result<PathBuf> {
    let emsdk_path = if let Some(path) = emsdk_override {
        path.to_path_buf()
    } else {
        let home = env::var("HOME").context("HOME environment variable not set")?;
        Path::new(&home).join("emsdk")
    };

    if !emsdk_path.exists() {
        eprintln!("⚠️  Emscripten SDK not found at: {}", emsdk_path.display());
        eprintln!("   Set --emsdk <path> or $EMSDK, or install it:");
        eprintln!("   https://emscripten.org/docs/getting_started/downloads.html");
        eprintln!();
        eprintln!("   Quick setup:");
        eprintln!("     git clone https://github.com/emscripten-core/emsdk.git ~/emsdk");
        eprintln!("     cd ~/emsdk && ./emsdk install latest && ./emsdk activate latest");
        bail!("Emscripten SDK not found");
    }

    let emsdk_env = emsdk_path.join("emsdk_env.sh");
    if !emsdk_env.exists() {
        bail!(
            "Invalid EMSDK directory (missing emsdk_env.sh): {}",
            emsdk_path.display()
        );
    }

    println!("📍 Using Emscripten SDK at: {}", emsdk_path.display());
    Ok(emsdk_path)
}

/// Copies the built artifacts from the target directory to the examples directory.
fn copy_artifacts(from: &Path, to: &Path) -> Result<()> {
    let artifacts = ["emscripten.js", "emscripten.wasm"];

    println!("📋 Copying artifacts to {}...", to.display());

    if !from.exists() {
        bail!(
            "Build output directory not found: {} (did the build succeed?)",
            from.display()
        );
    }

    fs::create_dir_all(to)
        .with_context(|| format!("Failed to create output directory {}", to.display()))?;

    for artifact in &artifacts {
        let src = from.join(artifact);
        let dst = to.join(artifact);

        if !src.exists() {
            bail!(
                "Expected build artifact not found: {} (missing {})",
                src.display(),
                artifact
            );
        }

        fs::copy(&src, &dst)
            .with_context(|| format!("Failed to copy {} to {}", src.display(), dst.display()))?;
        println!("   ✓ {artifact}");
    }

    Ok(())
}

fn bash_single_quote(value: &str) -> String {
    // Safely single-quote a string for use in a bash -c script.
    // Example: abc'def -> 'abc'\''def'
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Starts a local HTTP server to serve the `examples/web/` directory.
fn serve_examples(port: u16) -> Result<()> {
    let root = project_root()?;
    let web_dir = root.join("examples").join("web");

    println!();
    println!("🌐 Starting dev server at http://localhost:{port}");
    println!("   Press Ctrl+C to stop");
    println!();

    let status = Command::new("python3")
        .current_dir(&web_dir)
        .arg("-m")
        .arg("http.server")
        .arg(port.to_string())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to start HTTP server. Make sure Python 3 is installed.")?;

    if !status.success() {
        bail!("HTTP server exited with status: {status}");
    }

    Ok(())
}

/// Cleans build artifacts.
fn clean() -> Result<()> {
    let root = project_root()?;

    println!("🧹 Cleaning build artifacts...");

    // Clean cargo target
    let status = Command::new("cargo")
        .current_dir(&root)
        .arg("clean")
        .status()
        .context("Failed to run cargo clean")?;

    if !status.success() {
        bail!("cargo clean failed");
    }

    // Remove copied artifacts from examples/web/dist
    let dist_dir = root.join("examples").join("web").join("dist");
    let artifacts = ["emscripten.js", "emscripten.wasm"];

    for artifact in &artifacts {
        let path = dist_dir.join(artifact);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
            println!("   ✓ Removed examples/web/dist/{artifact}");
        }
    }

    // Back-compat: remove old copied artifacts in examples/ if present
    let legacy_examples_dir = root.join("examples");
    for artifact in &artifacts {
        let path = legacy_examples_dir.join(artifact);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
            println!("   ✓ Removed examples/{artifact}");
        }
    }

    println!("✅ Clean complete!");

    Ok(())
}

/// Delegate predefined CMap generation to the pdf-cmap helper binary.
fn generate_cmaps(source_dir: &Path, output: &Path) -> Result<()> {
    let root = project_root()?;

    let status = Command::new("cargo")
        .current_dir(&root)
        .arg("run")
        .arg("-p")
        .arg("pdf-cmap")
        .arg("--bin")
        .arg("generate-cmaps")
        .arg("--")
        .arg("--source-dir")
        .arg(source_dir)
        .arg("--output")
        .arg(output)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to execute pdf-cmap CMap generator")?;

    if !status.success() {
        bail!("pdf-cmap CMap generator failed with status: {status}");
    }

    Ok(())
}

fn build_web_canvas(profile: BuildProfile) -> Result<()> {
    let root = project_root()?;
    let version = Command::new("wasm-bindgen")
        .arg("--version")
        .output()
        .context("install wasm-bindgen-cli 0.2.100")?;
    if !version.status.success()
        || String::from_utf8_lossy(&version.stdout).trim() != "wasm-bindgen 0.2.100"
    {
        bail!(
            "this example requires wasm-bindgen-cli 0.2.100; run cargo install wasm-bindgen-cli --version 0.2.100 --locked"
        );
    }
    let mut build = Command::new("cargo");
    build.current_dir(&root).args([
        "build",
        "-p",
        "web-canvas-example",
        "--target",
        "wasm32-unknown-unknown",
    ]);
    if matches!(profile, BuildProfile::Release) {
        build.arg("--release");
    }
    let status = build.status().context("building Canvas 2D example")?;
    if !status.success() {
        bail!("Canvas 2D build failed: {status}");
    }
    let output = root.join("examples/web-canvas/pkg");
    let wasm = root
        .join("target/wasm32-unknown-unknown")
        .join(profile.as_target_dir_name())
        .join("web_canvas_example.wasm");
    let status = Command::new("wasm-bindgen")
        .arg(wasm)
        .args(["--target", "web", "--out-dir"])
        .arg(output)
        .status()
        .context("generating browser bindings")?;
    if !status.success() {
        bail!("wasm-bindgen failed: {status}");
    }
    for asset in [
        "annotation_layer.js",
        "annotation_layer.css",
        "annotation_dom.js",
        "annotation_models.js",
        "annotation_drag.js",
        "annotation_controls.js",
        "annotation_sessions.js",
        "annotation_notes.js",
        "annotation_visuals.js",
    ] {
        std::fs::copy(
            root.join("crates/pdf-web/js").join(asset),
            root.join("examples/web-canvas/pkg").join(asset),
        )
        .context("copying native annotation browser module")?;
    }
    println!(
        "Built examples/web-canvas. Run cargo xtask web-canvas --serve to open a local server."
    );
    Ok(())
}
