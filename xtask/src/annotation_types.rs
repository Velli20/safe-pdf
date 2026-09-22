//! Reproducible annotation declarations. Export is explicit, never a generated test.
use anyhow::{Context, Result, bail};
use pdf_annotation_core::{AnnotationCommandRequest, AnnotationReceipt, OptionalContentReceipt};
use pdf_web::WebAnnotationEntry;
use std::path::Path;
use ts_rs::TS;

const CONTRACT_FILE: &str = "annotation_contract.ts";

/// Generates into a scratch directory first so removed Rust types cannot survive
/// ts-rs's incremental file merging. Check mode leaves the checked-in file untouched.
pub(crate) fn generate(check: bool) -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("workspace root")?;
    let output = root.join("target/annotation-types");
    std::fs::create_dir_all(&output)?;
    let generated = output.join(CONTRACT_FILE);
    if generated.exists() {
        std::fs::remove_file(&generated)?;
    }
    let config = ts_rs::Config::new().with_out_dir(&output);
    WebAnnotationEntry::export_all(&config)?;
    AnnotationCommandRequest::export_all(&config)?;
    AnnotationReceipt::export_all(&config)?;
    OptionalContentReceipt::export_all(&config)?;
    // ts-rs emits spaces before newlines; normalize deterministically for review.
    let generated_text = std::fs::read_to_string(&generated)?;
    let contents = (generated_text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n")
        .into_bytes();
    let destination = root.join("crates/pdf-web/js").join(CONTRACT_FILE);
    if check {
        if std::fs::read(&destination).context("generate annotation types first")? != contents {
            bail!("annotation contract is stale; run cargo xtask annotation-types");
        }
    } else {
        std::fs::write(destination, contents)?;
    }
    Ok(())
}
