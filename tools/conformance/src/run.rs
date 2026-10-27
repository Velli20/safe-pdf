//! Runs a corpus: schedules isolated workers, derives verdicts, and writes reports.

use crate::{
    baseline::Baseline,
    corpus::{self, CorpusKind, Renderer, Target},
    model::{Case, CaseResult, PageOutput, PageResult, ReadOutput, Status},
    process, read_diff, report_case, report_index, signature,
    web_render::WebHost,
    worker,
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
};

/// A case after its read: the result so far and the pages still to render.
struct CaseRun {
    result: CaseResult,
    /// Evidence directory of the case.
    case_out: PathBuf,
    /// Pages to render; empty when the read already decided the case.
    pages: Vec<usize>,
    non_embedded_fonts: bool,
}

/// Runs `work` over `items` on `jobs` threads, each taking the next item when it is free.
/// Results come back in item order.
pub fn in_parallel<T: Sync, R: Send>(
    jobs: usize,
    items: &[T],
    work: impl Fn(usize, &T) -> R + Sync,
) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(items.len()));
    thread::scope(|scope| {
        for _ in 0..jobs.max(1).min(items.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    let result = work(index, item);
                    if let Ok(mut results) = results.lock() {
                        results.push((index, result));
                    }
                }
            });
        }
    });
    let mut results = results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

/// Returns the default number of parallel workers: one per core, leaving one free.
pub fn default_jobs() -> usize {
    thread::available_parallelism().map_or(4, |count| count.get().saturating_sub(1).max(1))
}

/// Settings of a corpus run.
pub struct RunOptions {
    /// Corpus kind.
    pub kind: CorpusKind,
    /// Renderer compared with the reference.
    pub renderer: Renderer,
    /// Chrome executable for the web renderer; `None` finds the installed one.
    pub chrome: Option<PathBuf>,
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
    /// Worker time and memory limits.
    pub limits: process::Limits,
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
    /// Returns the report this run writes.
    pub fn target(&self) -> Target {
        Target::new(self.kind, self.renderer)
    }

    /// Returns true when the run checks only part of the corpus, so absent failures say
    /// nothing about whether they were fixed.
    pub fn filtered(&self) -> bool {
        self.filter.is_some() || !self.cases.is_empty() || self.page.is_some()
    }

    /// Marks a signature with this run's renderer.
    pub fn tag(&self, signature: String) -> String {
        signature::tagged(signature, self.renderer)
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
        if self.renderer == Renderer::Web {
            args.extend(["--renderer".to_owned(), "web".to_owned()]);
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
    let out = corpus::output_dir(options.target());
    fs::create_dir_all(out.join("cases"))?;
    let web = match options.renderer {
        Renderer::Web if !options.read_only => Some(WebHost::start(options.chrome.clone())?),
        _ => None,
    };
    let total = cases.len();
    let read = AtomicUsize::new(0);
    let runs = in_parallel(options.jobs, &cases, |_, case| {
        let run = read_case(options, &out, case);
        let finished = read.fetch_add(1, Ordering::SeqCst).saturating_add(1);
        match &run {
            Ok(run) if run.pages.is_empty() => println!(
                "[{finished}/{total}] {} {}",
                run.result.status.as_str(),
                case.id
            ),
            Ok(_) => {}
            Err(error) => println!("[{finished}/{total}] harness error {}: {error:#}", case.id),
        }
        run
    })
    .into_iter()
    .collect::<Result<Vec<_>>>()?;

    // Every page is its own job, so the pages of a long document render side by side.
    let jobs: Vec<(usize, usize)> = runs
        .iter()
        .enumerate()
        .flat_map(|(index, run)| run.pages.iter().map(move |page| (index, *page)))
        .collect();
    let page_total = jobs.len();
    let rendered = AtomicUsize::new(0);
    let pages = in_parallel(options.jobs, &jobs, |_, &(index, page)| {
        let run = runs
            .get(index)
            .ok_or_else(|| anyhow!("page job without its case"))?;
        let result = run_page(options, run, page, web.as_ref());
        let finished = rendered.fetch_add(1, Ordering::SeqCst).saturating_add(1);
        let id = &run.result.case.id;
        match &result {
            Ok(result) => println!(
                "[{finished}/{page_total} pages] {} {id} page {page}",
                result.status.as_str()
            ),
            Err(error) => {
                println!(
                    "[{finished}/{page_total} pages] harness error {id} page {page}: {error:#}"
                )
            }
        }
        result.map(|result| (index, result))
    })
    .into_iter()
    .collect::<Result<Vec<_>>>()?;
    let mut grouped: Vec<Vec<PageResult>> = runs.iter().map(|_| Vec::new()).collect();
    for (index, page) in pages {
        if let Some(case_pages) = grouped.get_mut(index) {
            case_pages.push(page);
        }
    }
    let mut results: Vec<CaseResult> = runs
        .into_iter()
        .zip(grouped)
        .map(|(run, pages)| run.finish(pages))
        .collect();
    results.sort_by(|a, b| a.case.id.cmp(&b.case.id));

    drop(web);
    let baseline = Baseline::load(&corpus::baseline_path(options.target()))?;
    let index = report_index::build(options, &results, &baseline);
    for result in &results {
        report_case::write(options, &out, result, &index)?;
    }
    report_index::write(&out, &index)?;
    fs::write(out.join("results.json"), serde_json::to_vec(&results)?)?;
    read_diff::Reads::from_results(options.target(), &results)
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
            corpus::baseline_path(options.target()).display()
        );
        for line in regressions.iter().take(50) {
            println!("  {line}");
        }
    }
    let has_baseline = corpus::baseline_path(options.target()).is_file();
    if !has_baseline {
        println!(
            "No baseline at {}; regressions are reported but do not fail the run. Record one with `cargo conformance accept`.",
            corpus::baseline_path(options.target()).display()
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

/// Reads a case and picks the pages to render. A case decided by its read (missing file,
/// crash, timeout, `--read-only`) comes back without pages.
fn read_case(options: &RunOptions, out: &Path, case: &Case) -> Result<CaseRun> {
    let dir = case_dir(&case.id);
    let case_out = out.join("cases").join(&dir);
    if case_out.exists() {
        fs::remove_dir_all(&case_out)?;
    }
    let mut run = CaseRun {
        result: CaseResult {
            case: case.clone(),
            status: Status::Unavailable,
            sha256: None,
            md5_mismatch: false,
            read: None,
            read_process: None,
            signature: None,
            pages: Vec::new(),
            dir,
        },
        case_out,
        pages: Vec::new(),
        non_embedded_fonts: false,
    };
    let result = &mut run.result;
    let Ok(bytes) = fs::read(&case.path) else {
        return Ok(run);
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
    let (read, evidence) = process::execute::<ReadOutput>(command, options.limits)?;
    let Some(read) = read else {
        result.status = if evidence.timed_out {
            Status::Timeout
        } else {
            Status::Crash
        };
        result.signature = Some(options.tag(signature::process(result.status, &evidence)));
        result.read_process = Some(evidence);
        return Ok(run);
    };
    result.read_process = Some(evidence);
    if let Some(error) = &read.safe_error {
        result.status = Status::ReadError;
        result.signature = Some(options.tag(signature::error(Status::ReadError, error)));
    }
    if options.read_only {
        if result.status != Status::ReadError {
            result.status = Status::Pass;
        }
        result.read = Some(read);
        return Ok(run);
    }
    run.pages = selected_pages(options, case, &read);
    run.non_embedded_fonts = !read.inventory.non_embedded_fonts.is_empty();
    run.result.read = Some(read);
    Ok(run)
}

impl CaseRun {
    /// Completes the case with its rendered pages and removes the evidence of pages that
    /// did not fail, now that no page of the case is still writing it.
    fn finish(self, pages: Vec<PageResult>) -> CaseResult {
        let mut result = self.result;
        for mut page in pages {
            if result.status == Status::ReadError && page.status == Status::RenderError {
                page.status = Status::ReadError;
                page.signature.clone_from(&result.signature);
            }
            if !page.status.is_failure() {
                let _ = fs::remove_file(self.case_out.join(worker::streams_file(page.page)));
            }
            result.pages.push(page);
        }
        // Only failing pages keep evidence; a case without any gets no bundle.
        let _ = fs::remove_dir(&self.case_out);
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
        result
    }
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

/// Renders and compares one page of a read case in a worker.
fn run_page(
    options: &RunOptions,
    run: &CaseRun,
    page: usize,
    web: Option<&WebHost>,
) -> Result<PageResult> {
    let case = &run.result.case;
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
        .arg(&run.case_out);
    add_reference(&mut command, options, &run.result.dir);
    if let Some(password) = &case.password {
        command.arg("--password").arg(password);
    }
    if let Some(web) = web {
        add_web(&mut command, web)?;
    }
    let (output, process) = process::execute::<PageOutput>(command, options.limits)?;
    if let Some(web) = web
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
        Some(output) => page_verdict(output, options.tolerance, run.non_embedded_fonts),
    };
    let signature = signature.map(|signature| options.tag(signature));
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

/// Points a page worker at the browser and harness page of the web renderer.
pub fn add_web(command: &mut std::process::Command, web: &WebHost) -> Result<()> {
    let endpoints = web.endpoints()?;
    command
        .arg("--devtools")
        .arg(endpoints.devtools)
        .arg("--harness")
        .arg(endpoints.harness);
    Ok(())
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
