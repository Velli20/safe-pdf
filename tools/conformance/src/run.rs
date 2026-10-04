//! Runs a corpus: schedules isolated workers, derives verdicts, and writes reports.

use crate::{
    baseline::Baseline,
    corpus::{self, CorpusKind},
    model::{Case, CaseResult, PageOutput, PageResult, ReadOutput, Status},
    process, read_diff, report_case, report_index, signature, worker,
};
use anyhow::{Result, anyhow};
use md5::{Digest as _, Md5};
use sha2::Sha256;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

/// Settings of a corpus run.
pub struct RunOptions {
    /// Corpus kind.
    pub kind: CorpusKind,
    /// Corpus checkout.
    pub root: PathBuf,
    /// True when `root` was given explicitly and must appear in reproduction commands.
    pub explicit_root: bool,
    /// Substring filter on case ids.
    pub filter: Option<String>,
    /// Exact case ids; empty selects every case.
    pub cases: Vec<String>,
    /// Single page to check.
    pub page: Option<usize>,
    /// Parallel workers.
    pub jobs: usize,
    /// Pixels per PDF point.
    pub scale: f32,
    /// Largest raster side in pixels.
    pub max_side: u32,
    /// Pages checked per document.
    pub max_pages: usize,
    /// Worker time limit.
    pub timeout: Duration,
    /// Mismatch fraction treated as a pass.
    pub tolerance: f64,
    /// PDFium shared library; `None` uses reference images from files.
    pub pdfium: Option<PathBuf>,
    /// Directory of published reference images, one `<case dir>/pN-ref.png` per page;
    /// `None` with no PDFium uses the corpus's golden images.
    pub reference_images: Option<PathBuf>,
    /// Only read each document and count its pages; no page is rendered or compared.
    pub read_only: bool,
}

impl RunOptions {
    /// Returns true when the run checks only part of the corpus, so absent failures say
    /// nothing about whether they were fixed.
    pub fn filtered(&self) -> bool {
        self.filter.is_some() || !self.cases.is_empty() || self.page.is_some()
    }

    /// Returns the command that reruns one case, optionally one page.
    pub fn reproduce(&self, case: &str, page: Option<usize>) -> String {
        let mut args = vec![
            "cargo".to_owned(),
            "conformance".to_owned(),
            "run".to_owned(),
            "--corpus".to_owned(),
            self.kind.as_str().to_owned(),
        ];
        if self.explicit_root {
            args.extend(["--root".to_owned(), self.root.display().to_string()]);
        }
        args.extend(["--case".to_owned(), case.to_owned()]);
        if self.read_only {
            args.push("--read-only".to_owned());
        }
        if let Some(page) = page {
            args.extend(["--page".to_owned(), page.to_string()]);
        }
        args.iter()
            .map(|arg| {
                if arg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./=:".contains(c))
                {
                    arg.clone()
                } else {
                    format!("'{}'", arg.replace('\'', "'\\''"))
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Runs the selected cases and returns true when any regressed against the baseline.
pub fn run(options: &RunOptions) -> Result<bool> {
    let cases = corpus::select(
        options.kind.cases(&options.root)?,
        options.filter.as_deref(),
        &options.cases,
    )?;
    let out = corpus::output_dir(options.kind);
    fs::create_dir_all(out.join("cases"))?;
    let total = cases.len();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(total));
    thread::scope(|scope| {
        for _ in 0..options.jobs.max(1) {
            scope.spawn(|| {
                while let Some(case) = cases.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let result = run_case(options, &out, case);
                    let finished = done.fetch_add(1, Ordering::SeqCst).saturating_add(1);
                    match &result {
                        Ok(result) => println!(
                            "[{finished}/{total}] {} {}",
                            result.status.as_str(),
                            result.case.id
                        ),
                        Err(error) => {
                            println!("[{finished}/{total}] harness error {}: {error:#}", case.id)
                        }
                    }
                    if let Ok(mut results) = results.lock() {
                        results.push(result);
                    }
                }
            });
        }
    });
    let mut results = results
        .into_inner()
        .map_err(|_| anyhow!("result collection poisoned"))?
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    results.sort_by(|a, b| a.case.id.cmp(&b.case.id));

    let baseline = Baseline::load(&corpus::baseline_path(options.kind))?;
    let index = report_index::build(options, &results, &baseline);
    for result in &results {
        report_case::write(options, &out, result, &index)?;
    }
    report_index::write(&out, &index)?;
    fs::write(out.join("results.json"), serde_json::to_vec(&results)?)?;
    read_diff::Reads::from_results(options.kind, &results)
        .save(&out.join(read_diff::READS_FILE))?;
    let regressions = index.regressions();
    println!(
        "\n{}\nReport: {}\nTriage: {}",
        index.totals_line(),
        out.join("index.html").display(),
        out.join("TRIAGE.md").display()
    );
    if !regressions.is_empty() {
        println!(
            "{} regressions against {}:",
            regressions.len(),
            corpus::baseline_path(options.kind).display()
        );
        for line in regressions.iter().take(50) {
            println!("  {line}");
        }
    }
    let has_baseline = corpus::baseline_path(options.kind).is_file();
    if !has_baseline {
        println!(
            "No baseline at {}; regressions are reported but do not fail the run. Record one with `cargo conformance accept`.",
            corpus::baseline_path(options.kind).display()
        );
    }
    Ok(has_baseline && !regressions.is_empty())
}

/// Returns the directory name of a case: readable, bounded, and unique.
pub fn case_dir(id: &str) -> String {
    let readable: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(100)
        .collect();
    let digest = format!("{:x}", Sha256::digest(id.as_bytes()));
    format!("{readable}-{}", digest.get(..8).unwrap_or_default())
}

fn run_case(options: &RunOptions, out: &Path, case: &Case) -> Result<CaseResult> {
    let dir = case_dir(&case.id);
    let case_out = out.join("cases").join(&dir);
    if case_out.exists() {
        fs::remove_dir_all(&case_out)?;
    }
    let mut result = CaseResult {
        case: case.clone(),
        status: Status::Unavailable,
        sha256: None,
        md5_mismatch: false,
        read: None,
        read_process: None,
        signature: None,
        pages: Vec::new(),
        dir,
    };
    let Ok(bytes) = fs::read(&case.path) else {
        return Ok(result);
    };
    result.sha256 = Some(format!("{:x}", Sha256::digest(&bytes)));
    result.md5_mismatch = case
        .expected_md5
        .as_ref()
        .is_some_and(|md5| *md5 != format!("{:x}", Md5::digest(&bytes)));
    drop(bytes);

    let mut command = process::worker_command()?;
    command.arg("read").arg("--pdf").arg(&case.path);
    add_reference(&mut command, options, &result.dir);
    if let Some(password) = &case.password {
        command.arg("--password").arg(password);
    }
    let (read, evidence) = process::execute::<ReadOutput>(command, options.timeout)?;
    let Some(read) = read else {
        result.status = if evidence.timed_out {
            Status::Timeout
        } else {
            Status::Crash
        };
        result.signature = Some(signature::process(result.status, &evidence));
        result.read_process = Some(evidence);
        return Ok(result);
    };
    result.read_process = Some(evidence);
    if let Some(error) = &read.safe_error {
        result.status = Status::ReadError;
        result.signature = Some(signature::error(Status::ReadError, error));
    }
    if options.read_only {
        if result.status != Status::ReadError {
            result.status = Status::Pass;
        }
        result.read = Some(read);
        return Ok(result);
    }
    let pages = selected_pages(options, case, &read);
    let non_embedded_fonts = !read.inventory.non_embedded_fonts.is_empty();
    result.read = Some(read);
    for page in pages {
        let mut page = run_page(
            options,
            &case_out,
            case,
            &result.dir,
            page,
            non_embedded_fonts,
        )?;
        if result.status == Status::ReadError && page.status == Status::RenderError {
            page.status = Status::ReadError;
            page.signature.clone_from(&result.signature);
        }
        result.pages.push(page);
    }
    if result.status != Status::ReadError {
        let worst = result
            .pages
            .iter()
            .max_by_key(|page| page.status.severity())
            .map(|page| (page.status, page.signature.clone()));
        (result.status, result.signature) = match worst {
            Some((status, signature)) if status.is_failure() => (status, signature),
            Some(_)
                if result
                    .pages
                    .iter()
                    .all(|page| page.status == Status::NoReference) =>
            {
                (Status::NoReference, None)
            }
            Some(_)
                if result
                    .pages
                    .iter()
                    .any(|page| page.status == Status::FontSubstitution) =>
            {
                (Status::FontSubstitution, None)
            }
            _ => (Status::Pass, None),
        };
    }
    Ok(result)
}

/// Pages to check: the manifest range intersected with the document, capped by `max_pages`.
/// A document Safe-PDF cannot read still gets its first page rendered as the expected image.
fn selected_pages(options: &RunOptions, case: &Case, read: &ReadOutput) -> Vec<usize> {
    let count = read.safe_pages.or(read.reference_pages).unwrap_or(0);
    if let Some(page) = options.page {
        return if page < count { vec![page] } else { Vec::new() };
    }
    let first = case.first_page.unwrap_or(0);
    let last = case
        .last_page
        .unwrap_or(usize::MAX)
        .min(count.saturating_sub(1));
    let limit = if read.safe_error.is_some() {
        1
    } else {
        options.max_pages
    };
    if count == 0 || first > last {
        return Vec::new();
    }
    (first..=last).take(limit).collect()
}

fn run_page(
    options: &RunOptions,
    case_out: &Path,
    case: &Case,
    case_dir: &str,
    page: usize,
    non_embedded_fonts: bool,
) -> Result<PageResult> {
    let mut command = process::worker_command()?;
    command
        .arg("page")
        .arg("--pdf")
        .arg(&case.path)
        .arg("--page")
        .arg(page.to_string())
        .arg("--scale")
        .arg(options.scale.to_string())
        .arg("--max-side")
        .arg(options.max_side.to_string())
        .arg("--tolerance")
        .arg(options.tolerance.to_string())
        .arg("--out-dir")
        .arg(case_out);
    add_reference(&mut command, options, case_dir);
    if let Some(password) = &case.password {
        command.arg("--password").arg(password);
    }
    let (output, process) = process::execute::<PageOutput>(command, options.timeout)?;
    let (status, signature) = match &output {
        None => {
            let status = if process.timed_out {
                Status::Timeout
            } else {
                Status::Crash
            };
            (status, Some(signature::process(status, &process)))
        }
        Some(output) => page_verdict(output, options.tolerance, non_embedded_fonts),
    };
    if !status.is_failure() {
        // Only failing pages keep evidence; passing cases get no bundle.
        let _ = fs::remove_file(case_out.join(worker::streams_file(page)));
        let _ = fs::remove_dir(case_out);
    }
    Ok(PageResult {
        status,
        page,
        output,
        process,
        signature,
    })
}

/// Returns the page status with its signature.
pub fn page_verdict(
    output: &PageOutput,
    tolerance: f64,
    non_embedded_fonts: bool,
) -> (Status, Option<String>) {
    if let Some(error) = &output.safe_error {
        return (
            Status::RenderError,
            Some(signature::error(Status::RenderError, error)),
        );
    }
    match &output.metrics {
        None => (Status::NoReference, None),
        Some(metrics)
            if metrics.mismatch > tolerance
                && signature::is_font_substitution(output, non_embedded_fonts) =>
        {
            (Status::FontSubstitution, None)
        }
        Some(metrics) if metrics.mismatch > tolerance => (
            Status::Mismatch,
            Some(signature::mismatch(output, non_embedded_fonts)),
        ),
        Some(_) => (Status::Pass, None),
    }
}

/// Passes the reference to a worker: published images of the case, or the PDFium library.
fn add_reference(command: &mut std::process::Command, options: &RunOptions, case_dir: &str) {
    if let Some(images) = &options.reference_images {
        command.arg("--reference-images").arg(images.join(case_dir));
        return;
    }
    if let Some(library) = &options.pdfium {
        add_pdfium(command, library);
    }
}

/// Passes the PDFium library to a worker. A component build keeps its dependent shared
/// libraries next to `libpdfium`, so that directory joins the dynamic loader path.
pub fn add_pdfium(command: &mut std::process::Command, library: &Path) {
    command.arg("--pdfium").arg(library);
    if let Some(dir) = library.parent() {
        let variable = if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        command.env(variable, dir);
    }
}
