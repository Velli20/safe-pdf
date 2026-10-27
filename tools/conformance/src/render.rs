//! Renders pages of a local PDF in workers, without a corpus, and reports what happened.
//!
//! This is the quickest way to look at a single document: it times the Safe-PDF render steps
//! of each page, writes the Safe-PDF images and the pages' content-stream graphs, and compares
//! against a reference image or PDFium when one is available. Pages render in parallel.

use crate::{
    corpus::Renderer,
    model::{PageOutput, PageResult, ReadOutput, Status},
    pdfium_build, process, report_case, run, signature,
    web_render::WebHost,
};
use anyhow::{Result, anyhow, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

/// Arguments of `render`.
#[derive(clap::Args)]
pub struct RenderArgs {
    /// PDF file.
    pub pdf: PathBuf,
    /// Zero-based page; repeat for several pages (default: the first page).
    #[arg(long, conflicts_with = "all_pages")]
    pub page: Vec<usize>,
    /// Render every page of the document.
    #[arg(long, conflicts_with = "reference")]
    pub all_pages: bool,
    /// Pages rendered in parallel.
    #[arg(short, long, default_value_t = run::default_jobs())]
    pub jobs: usize,
    /// Document password.
    #[arg(long)]
    pub password: Option<String>,
    /// Reference image of the page, such as a PDFium render, to compare against; only with a
    /// single page.
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
    /// Renderer: `skia` draws page content only; `web` is the browser stack with its
    /// annotation layer, compared with PDFium's render of annotations and form fields.
    #[arg(long, value_enum, default_value_t)]
    pub renderer: Renderer,
    /// Chrome executable for the web renderer (default: the installed Google Chrome or
    /// Chromium).
    #[arg(long, env = "CHROME_PATH")]
    pub chrome: Option<PathBuf>,
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

/// One `render` invocation: the document, where its evidence goes, and what it is compared with.
struct Render<'a> {
    args: &'a RenderArgs,
    pdf: PathBuf,
    out: PathBuf,
    reference: Reference,
    /// The browser shared by every page of the web renderer.
    web: Option<WebHost>,
}

/// Renders the pages and prints their report; returns true when no page failed.
pub fn run(args: &RenderArgs) -> Result<bool> {
    if !(args.scale.is_finite() && args.scale > 0.0) {
        bail!("--scale must be positive");
    }
    if args.reference.is_some() && args.page.len() > 1 {
        bail!("--reference is the image of one page; pass a single --page");
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
    let mut render = Render {
        args,
        pdf,
        out,
        reference: Reference::select(args)?,
        web: None,
    };
    let pages = render.pages()?;
    if let Reference::Image(image) = &render.reference
        && let Some(page) = pages.first()
    {
        fs::copy(image, render.out.join(format!("p{page}-ref.png")))?;
    }
    render.web = match args.renderer {
        Renderer::Web => Some(WebHost::start(args.chrome.clone())?),
        Renderer::Skia => None,
    };
    let finished = AtomicUsize::new(0);
    let results = run::in_parallel(args.jobs, &pages, |_, &page| {
        let result = render.page(page);
        if pages.len() > 1 {
            let done = finished.fetch_add(1, Ordering::SeqCst).saturating_add(1);
            match &result {
                Ok(result) => println!(
                    "[{done}/{}] page {page}: {}",
                    pages.len(),
                    result.status.as_str()
                ),
                Err(error) => println!(
                    "[{done}/{}] page {page}: harness error {error:#}",
                    pages.len()
                ),
            }
        }
        result
    })
    .into_iter()
    .collect::<Result<Vec<_>>>()?;
    render.web = None;
    fs::write(
        render.out.join("report.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    let report = render.report(&results);
    fs::write(render.out.join("summary.md"), &report)?;
    print!("{report}");
    Ok(results.iter().all(|result| !result.status.is_failure()))
}

impl Render<'_> {
    /// Returns the pages to render: `--page`, every page with `--all-pages`, or the first.
    fn pages(&self) -> Result<Vec<usize>> {
        if !self.args.all_pages {
            return Ok(if self.args.page.is_empty() {
                vec![0]
            } else {
                self.args.page.clone()
            });
        }
        // Counted in a worker, so a document that crashes the reader cannot take this down.
        let mut command = process::worker_command()?;
        command.arg("read").arg("--pdf").arg(&self.pdf);
        if let Reference::Pdfium(library) = &self.reference {
            run::add_pdfium(&mut command, library);
        }
        if let Some(password) = &self.args.password {
            command.arg("--password").arg(password);
        }
        let (read, process) = process::execute::<ReadOutput>(command, self.limits())?;
        let read = read.ok_or_else(|| {
            anyhow!(
                "reading the document to count its pages {} after {} ms",
                process.ending(),
                process.elapsed_ms
            )
        })?;
        match read.safe_pages.or(read.reference_pages) {
            Some(count) if count > 0 => Ok((0..count).collect()),
            _ => bail!(
                "cannot count the pages: {}",
                read.safe_error
                    .map(|error| error.message)
                    .or(read.reference_error)
                    .unwrap_or_else(|| "the document has no pages".to_owned())
            ),
        }
    }

    fn limits(&self) -> process::Limits {
        process::Limits::new(
            Duration::from_secs(self.args.timeout),
            process::DEFAULT_MEMORY_LIMIT_MIB,
        )
    }

    /// Returns the worker command that renders and compares `page`.
    fn page_command(&self, page: usize) -> Result<std::process::Command> {
        let mut command = process::worker_command()?;
        command
            .arg("page")
            .arg("--pdf")
            .arg(&self.pdf)
            .arg("--page")
            .arg(page.to_string())
            .arg("--scale")
            .arg(self.args.scale.to_string())
            .arg("--max-side")
            .arg("3000")
            .arg("--tolerance")
            .arg(self.args.tolerance.to_string())
            .arg("--out-dir")
            .arg(&self.out)
            .arg("--write-safe");
        match &self.reference {
            Reference::Image(_) => {
                command.arg("--reference-images").arg(&self.out);
            }
            Reference::Pdfium(library) => run::add_pdfium(&mut command, library),
            Reference::None => {}
        }
        if let Some(password) = &self.args.password {
            command.arg("--password").arg(password);
        }
        if let Some(web) = &self.web {
            run::add_web(&mut command, web)?;
        }
        Ok(command)
    }

    /// Renders `page` in a worker and derives its verdict.
    fn page(&self, page: usize) -> Result<PageResult> {
        let (output, process) =
            process::execute::<PageOutput>(self.page_command(page)?, self.limits())?;
        if let Some(web) = &self.web
            && output.is_none()
        {
            web.close_tab(&process.stderr);
        }
        let (status, signature) = match &output {
            None => {
                let status = if process.timed_out {
                    Status::Timeout
                } else {
                    Status::Crash
                };
                (status, Some(signature::process(status, &process)))
            }
            Some(output) => run::page_verdict(output, self.args.tolerance, false),
        };
        Ok(PageResult {
            status,
            output,
            page,
            process,
            signature: signature.map(|signature| signature::tagged(signature, self.args.renderer)),
        })
    }

    /// Formats the report printed to the terminal and written as `summary.md`.
    fn report(&self, results: &[PageResult]) -> String {
        let failed = results
            .iter()
            .filter(|result| result.status.is_failure())
            .count();
        let mut lines = vec![
            match results {
                [result] => format!(
                    "# {} page {}: {}",
                    self.pdf.display(),
                    result.page,
                    result.status.as_str()
                ),
                _ => format!(
                    "# {}: {} pages, {failed} failed",
                    self.pdf.display(),
                    results.len()
                ),
            },
            String::new(),
            format!("- Reference: {}", self.reference.describe()),
            format!("- Renderer: {:?}", self.args.renderer),
        ];
        match results {
            [result] => lines.extend(page_lines(result)),
            _ => {
                for result in results {
                    lines.push(String::new());
                    lines.push(format!(
                        "## Page {}: {}",
                        result.page,
                        result.status.as_str()
                    ));
                    lines.push(String::new());
                    lines.extend(page_lines(result));
                }
                lines.push(String::new());
            }
        }
        let mut files: Vec<String> = fs::read_dir(&self.out)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "summary.md")
            .collect();
        files.sort();
        lines.push(format!("- Output in {}:", self.out.display()));
        lines.extend(files.iter().map(|file| format!("  - {file}")));
        let mut text = lines.join("\n");
        text.push('\n');
        text
    }
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

/// Describes how one page went: signature, render times, mismatch and errors.
fn page_lines(result: &PageResult) -> Vec<String> {
    let mut lines = Vec::new();
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
    lines
}
