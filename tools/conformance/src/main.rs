//! Renders PDF test corpora with Safe-PDF, compares them with PDFium, and writes evidence for
//! fixing differences.
//!
//! See `tools/conformance/README.md`.

mod attribution;
mod baseline;
mod compare;
mod corpus;
mod corpus_pdfium;
mod corpus_pdfjs;
mod fetch;
mod golden;
mod inventory;
mod issues;
mod model;
mod pdfium_build;
mod process;
mod reference;
mod regions;
mod report_case;
mod report_index;
mod run;
mod safe_render;
mod signature;
mod source_hints;
mod trace_backend;
mod worker;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use corpus::CorpusKind;
use std::{fs, path::PathBuf, process::ExitCode, time::Duration};

#[derive(Parser)]
#[command(about = "Compare Safe-PDF renders with PDFium over PDF test corpora")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check out a corpus at its pinned revision from its upstream repository.
    Fetch {
        #[arg(long, value_enum)]
        corpus: CorpusKind,
        /// Checkout location (default: target/conformance/corpora/<corpus>).
        #[arg(long)]
        root: Option<PathBuf>,
        /// Also download pdf.js files referenced by `.link` files.
        #[arg(long)]
        links: bool,
    },
    /// Check cases and write reports to target/conformance/<corpus>/.
    Run {
        #[arg(long, value_enum)]
        corpus: CorpusKind,
        /// Corpus checkout (default: target/conformance/corpora/<corpus>).
        #[arg(long)]
        root: Option<PathBuf>,
        /// Only cases whose id contains this text.
        #[arg(long)]
        filter: Option<String>,
        /// Only the case with exactly this id.
        #[arg(long, conflicts_with = "filter")]
        case: Option<String>,
        /// Only this zero-based page.
        #[arg(long)]
        page: Option<usize>,
        /// Parallel workers.
        #[arg(short, long, default_value_t = default_jobs())]
        jobs: usize,
        /// Pixels per PDF point.
        #[arg(long, default_value_t = 1.5)]
        scale: f32,
        /// Largest raster side; larger pages are scaled down.
        #[arg(long, default_value_t = 3000)]
        max_side: u32,
        /// Pages checked per document.
        #[arg(long, default_value_t = 10)]
        max_pages: usize,
        /// Seconds before a worker is killed.
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// Fraction of mismatched pixels still counted as a pass.
        #[arg(long, default_value_t = 0.002)]
        tolerance: f64,
        /// Reference images (default: goldens for pdfium, pdfium for pdfjs).
        #[arg(long, value_enum)]
        reference: Option<ReferenceKind>,
        /// PDFium shared library (default: the one built by `setup-pdfium`).
        #[arg(long, env = "PDFIUM_LIBRARY")]
        pdfium: Option<PathBuf>,
    },
    /// Build PDFium from official sources for the `pdfium` reference (large, one-time).
    SetupPdfium,
    /// File a GitHub issue per unreported failure cluster of the last run (uses `gh`).
    Issues {
        #[arg(long, value_enum)]
        corpus: CorpusKind,
        /// Repository as owner/name.
        #[arg(long, env = "GITHUB_REPOSITORY")]
        repo: String,
        /// Issues created at most per run; further clusters wait for later runs.
        #[arg(long, default_value_t = 15)]
        max_new: usize,
        /// Print planned actions and write bodies under target/conformance/<corpus>/issues/.
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the summary of a case from the last run.
    Show {
        #[arg(long, value_enum)]
        corpus: CorpusKind,
        /// Case id.
        case: String,
    },
    /// Record the last run's outcomes as the expected baseline.
    Accept {
        #[arg(long, value_enum)]
        corpus: CorpusKind,
        /// Only record this case.
        #[arg(long)]
        case: Option<String>,
    },
    #[command(hide = true, subcommand)]
    Worker(WorkerCommand),
}

#[derive(Subcommand)]
enum WorkerCommand {
    Read {
        #[arg(long)]
        pdf: PathBuf,
        #[arg(long)]
        password: Option<String>,
        #[arg(long)]
        pdfium: Option<PathBuf>,
    },
    Page {
        #[arg(long)]
        pdf: PathBuf,
        #[arg(long)]
        password: Option<String>,
        #[arg(long)]
        pdfium: Option<PathBuf>,
        #[arg(long)]
        page: usize,
        #[arg(long)]
        scale: f32,
        #[arg(long)]
        max_side: u32,
        #[arg(long)]
        tolerance: f64,
        #[arg(long)]
        out_dir: PathBuf,
    },
}

/// Source of the reference images.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum ReferenceKind {
    /// Official golden PNGs shipped with pdfium_tests; needs no PDFium library.
    Goldens,
    /// Pages rendered by a PDFium shared library.
    Pdfium,
}

fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(4, |count| count.get().saturating_sub(1).max(1))
}

fn main() -> Result<ExitCode> {
    match Cli::parse().command {
        Command::Fetch {
            corpus,
            root,
            links,
        } => fetch::run(corpus, root.as_deref(), links)?,
        Command::Run {
            corpus,
            root,
            filter,
            case,
            page,
            jobs,
            scale,
            max_side,
            max_pages,
            timeout,
            tolerance,
            reference,
            pdfium,
        } => {
            if !(scale.is_finite() && scale > 0.0) {
                bail!("--scale must be positive");
            }
            let reference = reference.unwrap_or(match corpus {
                CorpusKind::Pdfium => ReferenceKind::Goldens,
                CorpusKind::Pdfjs => ReferenceKind::Pdfium,
            });
            let pdfium = match reference {
                ReferenceKind::Goldens if corpus == CorpusKind::Pdfjs => {
                    bail!("the pdf.js corpus has no golden images; use --reference pdfium")
                }
                ReferenceKind::Goldens => None,
                ReferenceKind::Pdfium => Some(pdfium_library(pdfium)?),
            };
            let options = run::RunOptions {
                kind: corpus,
                explicit_root: root.is_some(),
                root: {
                    let root = root.unwrap_or_else(|| corpus::default_root(corpus));
                    root.canonicalize()
                        .or_else(|_| std::path::absolute(&root))?
                },
                filter,
                case,
                page,
                jobs,
                scale,
                max_side,
                max_pages,
                timeout: Duration::from_secs(timeout),
                tolerance,
                pdfium,
            };
            if run::run(&options)? {
                return Ok(ExitCode::FAILURE);
            }
        }
        Command::Show { corpus, case } => {
            let summary = corpus::output_dir(corpus)
                .join("cases")
                .join(run::case_dir(&case))
                .join("summary.md");
            match fs::read_to_string(&summary) {
                Ok(text) => print!("{text}"),
                Err(_) => bail!(
                    "no summary for {case}; it passed or was not part of the last run ({})",
                    summary.display()
                ),
            }
        }
        Command::Accept { corpus, case } => accept(corpus, case.as_deref())?,
        Command::SetupPdfium => pdfium_build::run()?,
        Command::Issues {
            corpus,
            repo,
            max_new,
            dry_run,
        } => issues::run(&issues::IssueOptions {
            kind: corpus,
            repo: &repo,
            max_new,
            dry_run,
        })?,
        Command::Worker(WorkerCommand::Read {
            pdf,
            password,
            pdfium,
        }) => worker::read(&pdf, password.as_deref(), pdfium.as_deref())?,
        Command::Worker(WorkerCommand::Page {
            pdf,
            password,
            pdfium,
            page,
            scale,
            max_side,
            tolerance,
            out_dir,
        }) => worker::page(&worker::PageJob {
            pdf: &pdf,
            password: password.as_deref(),
            page,
            scale,
            max_side,
            tolerance,
            out_dir: &out_dir,
            pdfium: pdfium.as_deref(),
        })?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Resolves the PDFium library: `--pdfium`/`PDFIUM_LIBRARY`, else the `setup-pdfium` build.
fn pdfium_library(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let library = match explicit {
        Some(path) => std::path::absolute(path)?,
        None => match pdfium_build::built_library() {
            Some(path) => path,
            None => bail!(
                "no PDFium library: run `cargo conformance setup-pdfium` once to build it from \
                 official sources, or pass --pdfium / PDFIUM_LIBRARY"
            ),
        },
    };
    if !library.is_file() {
        bail!("PDFium library {} not found", library.display());
    }
    Ok(library)
}

fn accept(kind: CorpusKind, case: Option<&str>) -> Result<()> {
    let results_path = corpus::output_dir(kind).join("results.json");
    let Ok(bytes) = fs::read(&results_path) else {
        bail!(
            "no results at {}; run the corpus first",
            results_path.display()
        );
    };
    let results: Vec<model::CaseResult> = serde_json::from_slice(&bytes)?;
    let path = corpus::baseline_path(kind);
    let mut baseline = baseline::Baseline::load(&path)?;
    let index: report_index::Index =
        serde_json::from_slice(&fs::read(corpus::output_dir(kind).join("index.json"))?)?;
    baseline.pdfium = index.pdfium;
    if let Some(root) = results.first().and_then(|result| result.case.path.parent()) {
        baseline.corpus_revision = root.ancestors().find_map(fetch::revision);
    }
    let mut recorded = 0usize;
    for result in results
        .iter()
        .filter(|result| case.is_none_or(|id| result.case.id == id))
    {
        baseline.record(result);
        recorded = recorded.saturating_add(1);
    }
    if recorded == 0 {
        bail!("no matching case in the last run");
    }
    baseline.save(&path)?;
    println!("recorded {recorded} cases in {}", path.display());
    Ok(())
}
