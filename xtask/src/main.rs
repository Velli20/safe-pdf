//! Build automation tasks for safe-pdf.
//!
//! Usage:
//!   cargo xtask emscripten [--profile release|debug] [--features skia-wasm] [--serve] [--port 8080] [--emsdk <path>]
//!
//! This crate provides idiomatic Rust tooling for building and packaging
//! the emscripten WebAssembly example.

use anyhow::{Context, Result, bail};
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

        /// Start a local dev server after building
        #[arg(long)]
        serve: bool,

        /// Port for the dev server
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Clean build artifacts
    Clean,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Emscripten {
            profile,
            features,
            emsdk,
            serve,
            port,
        } => {
            build_emscripten(profile, &features, emsdk.as_deref())?;
            if serve {
                serve_examples(port)?;
            }
        }
        Commands::Clean => {
            clean()?;
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

/// Builds the emscripten example and copies artifacts to `examples/web/dist/`.
fn build_emscripten(
    profile: BuildProfile,
    features: &str,
    emsdk_override: Option<&Path>,
) -> Result<()> {
    let root = project_root()?;
    let dist_dir = root.join("examples").join("web").join("dist");

    println!("🔧 Building emscripten example...");

    // Check emsdk exists
    let emsdk_path = get_emsdk_path(emsdk_override)?;

    // Set EMCC_CFLAGS
    let emcc_cflags = [
        "--no-entry",
        "-sASSERTIONS=1",
        "-sALLOW_TABLE_GROWTH=1",
        "-sALLOW_MEMORY_GROWTH=1",
        "-sENVIRONMENT=web",
        "-sERROR_ON_UNDEFINED_SYMBOLS=0",
        "-sMAX_WEBGL_VERSION=2",
    ]
    .join(" ");

    // Set RUSTFLAGS for exported functions
    let rustflags = [
        "-C link-args=-sEXPORTED_FUNCTIONS=['_sk_load_pdf','_sk_get_page_count','_sk_render_page','_sk_free_pdf','_malloc','_free','_main']",
        "-C link-args=-sEXPORTED_RUNTIME_METHODS=['cwrap','HEAPU8']",
        "-C link-args=-sSTANDALONE_WASM=0",
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
    }

    println!("📦 Running: cargo {}", cargo_args.join(" "));

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
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to execute cargo build")?;

    if !status.success() {
        bail!("Cargo build failed with status: {}", status);
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
        println!("   ✓ {}", artifact);
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
    println!("🌐 Starting dev server at http://localhost:{}", port);
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
        bail!("HTTP server exited with status: {}", status);
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
            println!("   ✓ Removed examples/web/dist/{}", artifact);
        }
    }

    // Back-compat: remove old copied artifacts in examples/ if present
    let legacy_examples_dir = root.join("examples");
    for artifact in &artifacts {
        let path = legacy_examples_dir.join(artifact);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
            println!("   ✓ Removed examples/{}", artifact);
        }
    }

    println!("✅ Clean complete!");

    Ok(())
}
