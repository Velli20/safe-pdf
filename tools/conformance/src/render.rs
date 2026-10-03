//! Renders one page of a local PDF in a worker, without a corpus, and reports what happened.
//!
//! This is the quickest way to look at a single document: it times both Safe-PDF render
//! steps, writes the Safe-PDF image and the page's content-stream graph, and compares against
//! a reference image or PDFium when one is available.

use crate::{
    model::{PageOutput, PageResult, Status},
    pdfium_build, process, report_case, run,
};
use anyhow::{Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// Arguments of `render`.
#[derive(clap::Args)]
pub struct RenderArgs {
    /// PDF file.
    pub pdf: PathBuf,
    /// Zero-based page.
    #[arg(long, default_value_t = 0)]
    pub page: usize,
    /// Document password.
    #[arg(long)]
    pub password: Option<String>,
    /// Reference image of the page, such as a PDFium render, to compare against.
    #[arg(long, conflicts_with = "pdfium")]
    pub reference: Option<PathBuf>,
    /// PDFium shared library to render the reference with (default: the one built by
    /// `setup-pdfium`, when present).
    #[arg(long, env = "PDFIUM_LIBRARY")]
    pub pdfium: Option<PathBuf>,
    /// Pixels per PDF point; a reference image fixes its own size.
    #[arg(long, default_value_t = 1.5)]
    pub scale: f32,
    /// Seconds before the worker is killed.
    #[arg(long, default_value_t = 60)]
    pub timeout: u64,
    /// Fraction of mismatched pixels still counted as a pass.
    #[arg(long, default_value_t = 0.002)]
    pub tolerance: f64,
    /// Output directory (default: target/conformance/render/<file name>).
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// Where the reference image of the page comes from.
enum Reference {
    /// An image file copied into the output directory as `pN-ref.png`.
    Image(PathBuf),
    /// A PDFium library rendering the page.
    Pdfium(PathBuf),
    /// Nothing to compare against; only Safe-PDF runs.
    None,
}

impl Reference {
    /// Picks the reference: an explicit image, an explicit library, or a local PDFium build.
    fn select(args: &RenderArgs) -> Result<Self> {
        if let Some(image) = &args.reference {
            return Ok(Self::Image(std::path::absolute(image)?));
        }
        match args.pdfium.clone().or_else(pdfium_build::built_library) {
            Some(library) if library.is_file() => Ok(Self::Pdfium(std::path::absolute(library)?)),
            Some(library) => bail!("PDFium library {} not found", library.display()),
            None => Ok(Self::None),
        }
    }

    /// Describes the reference for the report.
    fn describe(&self) -> String {
        match self {
            Self::Image(path) => format!("image {}", path.display()),
            Self::Pdfium(path) => format!("PDFium {}", path.display()),
            Self::None => "none; only Safe-PDF ran".to_owned(),
        }
    }
}

/// Renders the page and prints its report; returns true when the page did not fail.
pub fn run(args: &RenderArgs) -> Result<bool> {
    if !(args.scale.is_finite() && args.scale > 0.0) {
        bail!("--scale must be positive");
    }
    let pdf = std::path::absolute(&args.pdf)?;
    if !pdf.is_file() {
        bail!("{} not found", pdf.display());
    }
    let out = match &args.out {
        Some(out) => std::path::absolute(out)?,
        None => default_out(&pdf),
    };
    if out.exists() {
        fs::remove_dir_all(&out)?;
    }
    fs::create_dir_all(&out)?;
    let reference = Reference::select(args)?;

    let mut command = process::worker_command()?;
    command
        .arg("page")
        .arg("--pdf")
        .arg(&pdf)
        .arg("--page")
        .arg(args.page.to_string())
        .arg("--scale")
        .arg(args.scale.to_string())
        .arg("--max-side")
        .arg("3000")
        .arg("--tolerance")
        .arg(args.tolerance.to_string())
        .arg("--out-dir")
        .arg(&out)
        .arg("--write-safe");
    match &reference {
        Reference::Image(image) => {
            fs::copy(image, out.join(format!("p{}-ref.png", args.page)))?;
            command.arg("--reference-images").arg(&out);
        }
        Reference::Pdfium(library) => run::add_pdfium(&mut command, library),
        Reference::None => {}
    }
    if let Some(password) = &args.password {
        command.arg("--password").arg(password);
    }
    let (output, process) =
        process::execute::<PageOutput>(command, Duration::from_secs(args.timeout))?;
    let (status, signature) = match &output {
        None => {
            let status = if process.timed_out {
                Status::Timeout
            } else {
                Status::Crash
            };
            (status, Some(crate::signature::process(status, &process)))
        }
        Some(output) => run::page_verdict(output, args.tolerance, false),
    };
    let result = PageResult {
        status,
        output,
        page: args.page,
        process,
        signature,
    };
    fs::write(out.join("report.json"), serde_json::to_vec_pretty(&result)?)?;
    let report = report(&pdf, &reference, &result, &out);
    fs::write(out.join("summary.md"), &report)?;
    print!("{report}");
    Ok(!status.is_failure())
}

/// Returns `target/conformance/render/<file name>` for a PDF.
fn default_out(pdf: &Path) -> PathBuf {
    let name = pdf
        .file_name()
        .map_or_else(|| "document".into(), |name| name.to_string_lossy());
    crate::corpus::state_dir()
        .join("render")
        .join(run::case_dir(&name))
}

/// Formats the page report printed to the terminal and written as `summary.md`.
fn report(pdf: &Path, reference: &Reference, result: &PageResult, out: &Path) -> String {
    let mut lines = vec![
        format!(
            "# {} page {}: {}",
            pdf.display(),
            result.page,
            result.status.as_str()
        ),
        String::new(),
        format!("- Reference: {}", reference.describe()),
    ];
    if let Some(signature) = &result.signature {
        lines.push(format!("- Signature: `{signature}`"));
    }
    if let Some(line) = report_case::render_line(&result.process) {
        lines.push(format!("- Safe-PDF render: {line}"));
    }
    match &result.output {
        None => lines.push(format!(
            "- Worker {} after {} ms in stage `{}` ({})",
            if result.process.timed_out {
                "timed out"
            } else {
                "crashed"
            },
            result.process.elapsed_ms,
            result.process.stage.as_deref().unwrap_or("startup"),
            result.process.exit_status
        )),
        Some(output) => {
            if let Some(metrics) = &output.metrics {
                lines.push(format!(
                    "- Mismatch {:.3}% of pixels; {} x {} px",
                    metrics.mismatch * 100.0,
                    output.size[0],
                    output.size[1]
                ));
            }
            if let Some(error) = &output.reference_error {
                lines.push(format!("- No reference image: {error}"));
            }
            if let Some(error) = &output.safe_error {
                lines.push(format!("- Safe-PDF error: {}", error.message));
                lines.extend(error.chain.iter().map(|cause| format!("  - {cause}")));
            }
        }
    }
    let mut files: Vec<String> = fs::read_dir(out)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "summary.md")
        .collect();
    files.sort();
    lines.push(format!("- Output in {}:", out.display()));
    lines.extend(files.iter().map(|file| format!("  - {file}")));
    let mut text = lines.join("\n");
    text.push('\n');
    text
}
