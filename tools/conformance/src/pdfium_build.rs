//! Builds PDFium from official sources: depot_tools from chromium.googlesource.com and
//! PDFium from pdfium.googlesource.com. Nothing prebuilt is downloaded by this harness;
//! depot_tools itself fetches Google's pinned toolchain (clang, gn, ninja) during sync.

use crate::corpus;
use anyhow::{Context, Result, bail};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const DEPOT_TOOLS_URL: &str = "https://chromium.googlesource.com/chromium/tools/depot_tools";
/// depot_tools `main` when the harness was pinned.
const DEPOT_TOOLS_REVISION: &str = "41c9bd890277c2f551499d171d215dfdf5dab97d";
const PDFIUM_URL: &str = "https://pdfium.googlesource.com/pdfium.git";
/// `refs/heads/chromium/7881`, matching the `pdfium_7881` API of `pdfium-render`.
const PDFIUM_REVISION: &str = "91b9d569b34be4f38eed7b3c49b227356c3aadad";
/// Release shared library without V8/XFA. The component build makes `//:pdfium` a shared
/// library; Skia matches Safe-PDF's rasterizer and PDFium's preferred golden images.
const GN_ARGS: &str = "is_debug=false is_component_build=true pdf_is_standalone=true \
    pdf_enable_v8=false pdf_enable_xfa=false pdf_use_skia=true treat_warnings_as_errors=false";

fn build_dir() -> PathBuf {
    corpus::state_dir().join("pdfium-build")
}

fn library_record() -> PathBuf {
    build_dir().join("library.txt")
}

/// Returns the library recorded by a completed `setup-pdfium`.
pub fn built_library() -> Option<PathBuf> {
    let path = PathBuf::from(fs::read_to_string(library_record()).ok()?.trim());
    path.is_file().then_some(path)
}

/// Runs every build step; completed steps are skipped or finish quickly on reruns.
pub fn run() -> Result<()> {
    let base = build_dir();
    fs::create_dir_all(&base)?;
    let depot_tools = base.join("depot_tools");
    let checkout = base.join("pdfium");
    let out = checkout.join("out").join("Shared");
    println!(
        "Building PDFium {PDFIUM_REVISION} in {}.\nThe first run downloads several GB of sources and toolchain and can take a long time.",
        base.display()
    );

    if !depot_tools.join(".git").exists() {
        step(
            "clone depot_tools",
            git(&base).args(["clone", "-q", DEPOT_TOOLS_URL, "depot_tools"]),
        )?;
    }
    step(
        "pin depot_tools",
        git(&depot_tools).args(["checkout", "-q", DEPOT_TOOLS_REVISION]),
    )?;

    let path = tool_path(&depot_tools)?;
    // With self-updates disabled, depot_tools does not bootstrap its bundled Python and
    // CIPD tools on its own; its wrappers (gn, autoninja) need them.
    if !depot_tools.join("python3_bin_reldir.txt").is_file() {
        step(
            "bootstrap depot_tools",
            &mut tool(&path, &depot_tools, "./ensure_bootstrap"),
        )?;
    }
    if !base.join(".gclient").is_file() {
        step(
            "gclient config",
            tool(&path, &base, "gclient").args(["config", "--unmanaged", PDFIUM_URL]),
        )?;
    }
    let stamp = base.join("synced-revision.txt");
    if fs::read_to_string(&stamp).ok().as_deref().map(str::trim) != Some(PDFIUM_REVISION) {
        step(
            "gclient sync",
            tool(&path, &base, "gclient")
                .args(["sync", "--no-history", "--shallow", "--revision"])
                .arg(format!("pdfium@{PDFIUM_REVISION}")),
        )?;
        fs::write(&stamp, PDFIUM_REVISION)?;
    }
    step(
        "gn gen",
        tool(&path, &checkout, "gn")
            .args(["gen", "out/Shared"])
            .arg(format!("--args={GN_ARGS}")),
    )?;
    step(
        "ninja",
        tool(&path, &checkout, "autoninja").args(["-C", "out/Shared", "pdfium"]),
    )?;

    let library = pdfium_render::prelude::Pdfium::pdfium_platform_library_name_at_path(&out);
    if !library.is_file() {
        bail!(
            "the build finished but {} is missing; inspect {}",
            library.display(),
            out.display()
        );
    }
    fs::write(library_record(), library.display().to_string())?;
    println!(
        "PDFium library: {}\nNext: cargo conformance run --corpus pdfjs",
        library.display()
    );
    Ok(())
}

fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(dir);
    command
}

/// Returns `PATH` with depot_tools first, for child processes only.
fn tool_path(depot_tools: &Path) -> Result<OsString> {
    let mut paths = vec![depot_tools.to_owned()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    Ok(std::env::join_paths(paths)?)
}

fn tool(path: &OsString, dir: &Path, name: &str) -> Command {
    let mut command = Command::new(name);
    command
        .current_dir(dir)
        .env("PATH", path)
        // Keep depot_tools at its pinned revision.
        .env("DEPOT_TOOLS_UPDATE", "0");
    command
}

fn step(name: &str, command: &mut Command) -> Result<()> {
    println!("==> {name}: {command:?}");
    let status = command
        .status()
        .with_context(|| format!("launching {name}"))?;
    if !status.success() {
        bail!(
            "{name} failed ({status}); fix the cause and rerun `cargo conformance setup-pdfium` to resume"
        );
    }
    Ok(())
}
