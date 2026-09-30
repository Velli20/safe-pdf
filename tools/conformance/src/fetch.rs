//! Checks out pinned corpora from their upstream repositories and downloads linked pdf.js files.
//!
//! No binaries are downloaded here; `setup-pdfium` builds PDFium from official sources.

use crate::corpus::{self, CorpusKind};
use anyhow::{Context, Result, bail};
use md5::{Digest as _, Md5};
use sha2::Sha256;
use std::{collections::BTreeSet, fs, path::Path, process::Command};

/// Describes the PDFium library in reports: its file name and a SHA-256 prefix, so runs and
/// baselines recorded with different builds can be told apart.
pub fn pdfium_identity(library: &Path) -> Result<String> {
    let digest = format!("{:x}", Sha256::digest(fs::read(library)?));
    Ok(format!(
        "{} (sha256 {})",
        library
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        digest.get(..16).unwrap_or_default()
    ))
}

/// Fetches a corpus at its pinned revision and, optionally, its linked files, then prints
/// the command to run next.
pub fn run(kind: CorpusKind, root: Option<&Path>, links: bool) -> Result<()> {
    let explicit = root.is_some();
    let root = root.map_or_else(|| corpus::default_root(kind), Path::to_owned);
    checkout(kind, &root, explicit)?;
    if links && kind == CorpusKind::Pdfjs {
        fetch_links(&root, None)?;
    } else if kind == CorpusKind::Pdfjs {
        println!(
            "Linked pdf.js files are not downloaded; add --links to fetch them from their original URLs."
        );
    }
    let root_arg = if explicit {
        format!(" --root {}", root.display())
    } else {
        String::new()
    };
    match kind {
        CorpusKind::Pdfium => println!(
            "Next: cargo conformance run --corpus pdfium{root_arg}   (compares against the corpus's official golden images; no PDFium build needed)"
        ),
        CorpusKind::Pdfjs => {
            if crate::pdfium_build::built_library().is_none() {
                println!(
                    "The pdf.js corpus needs a PDFium reference: run `cargo conformance setup-pdfium` once."
                );
            }
            println!("Next: cargo conformance run --corpus pdfjs{root_arg}");
        }
    }
    Ok(())
}

/// Clones the corpus at its pinned revision. A user-provided checkout at another revision
/// is left untouched with a warning.
fn checkout(kind: CorpusKind, root: &Path, explicit: bool) -> Result<()> {
    let (url, revision) = kind.source();
    if root.join(".git").exists() {
        let head = git_output(root, &["rev-parse", "HEAD"])?;
        if head == revision {
            println!(
                "{} corpus already at pinned revision {} in {}; nothing to download.",
                kind.as_str(),
                revision.get(..12).unwrap_or(revision),
                root.display()
            );
            return Ok(());
        }
        if explicit {
            println!(
                "Warning: {} is at {} but the pinned revision is {revision}; leaving your checkout unchanged. \
                 Results and baselines may differ from the pinned corpus.",
                root.display(),
                head.get(..12).unwrap_or(&head)
            );
            return Ok(());
        }
    } else {
        println!(
            "Cloning {} corpus at {revision} into {}",
            kind.as_str(),
            root.display()
        );
        fs::create_dir_all(root)?;
        run_command(
            Command::new("git")
                .arg("-C")
                .arg(root)
                .arg("init")
                .arg("-q"),
        )?;
        run_command(
            Command::new("git")
                .arg("-C")
                .arg(root)
                .args(["remote", "add", "origin", url]),
        )?;
    }
    if let Some(paths) = kind.sparse_paths() {
        run_command(
            Command::new("git")
                .arg("-C")
                .arg(root)
                .args(["sparse-checkout", "set"])
                .args(paths),
        )?;
    }
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["fetch", "-q", "--depth", "1", "origin", revision]),
    )?;
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["checkout", "-q", "FETCH_HEAD"]),
    )?;
    println!("{} corpus at {revision}: {}", kind.as_str(), root.display());
    Ok(())
}

/// Checks out only `paths` of a corpus at `revision` into `root`, fetching file contents on
/// demand, so reproducing a few cases does not download the whole corpus. Calling it again
/// adds paths to the checkout.
pub fn sparse_checkout(
    kind: CorpusKind,
    root: &Path,
    revision: &str,
    paths: &[String],
) -> Result<()> {
    let (url, _) = kind.source();
    let git = |args: &[&str]| run_command(Command::new("git").arg("-C").arg(root).args(args));
    if !root.join(".git").exists() {
        fs::create_dir_all(root)?;
        git(&["init", "-q"])?;
        git(&["remote", "add", "origin", url])?;
        git(&["config", "remote.origin.promisor", "true"])?;
        git(&["config", "remote.origin.partialclonefilter", "blob:none"])?;
        git(&["sparse-checkout", "set", "--no-cone", "/.gitignore"])?;
    }
    if revision_of(root).as_deref() != Some(revision) {
        git(&[
            "fetch",
            "-q",
            "--depth",
            "1",
            "--filter=blob:none",
            "origin",
            revision,
        ])?;
        git(&["checkout", "-q", "FETCH_HEAD"])?;
    }
    let mut args = vec!["sparse-checkout", "add"];
    let patterns: Vec<String> = paths
        .iter()
        .map(|path| {
            // Sparse patterns use gitignore syntax; escape its special characters.
            let escaped: String = path
                .chars()
                .flat_map(|c| {
                    let escape = matches!(c, '*' | '?' | '[' | ']' | '!' | '#' | '\\' | ' ');
                    escape.then_some('\\').into_iter().chain(std::iter::once(c))
                })
                .collect();
            format!("/{escaped}")
        })
        .collect();
    args.extend(patterns.iter().map(String::as_str));
    git(&args)
}

fn revision_of(root: &Path) -> Option<String> {
    git_output(root, &["rev-parse", "HEAD"]).ok()
}

/// Parallel downloads of pdf.js linked files.
const LINK_WORKERS: usize = 8;

/// Downloads pdf.js `.link` files, keeping only downloads whose MD5 matches the manifest.
/// `only` limits the downloads to these test ids.
pub fn fetch_links(root: &Path, only: Option<&BTreeSet<String>>) -> Result<()> {
    let mut pending = Vec::new();
    let (mut present, mut failed) = (0usize, Vec::new());
    for entry in crate::corpus_pdfjs::manifest(root)? {
        if !entry.link || only.is_some_and(|ids| !ids.contains(&entry.id)) {
            continue;
        }
        let pdf = root.join("test").join(&entry.file);
        if pdf.is_file() {
            present = present.saturating_add(1);
            continue;
        }
        match fs::read_to_string(format!("{}.link", pdf.display())) {
            Ok(url) => pending.push((entry.id, url.trim().to_owned(), pdf, entry.md5)),
            Err(_) => failed.push(format!("{}: missing .link file", entry.id)),
        }
    }
    println!(
        "downloading {} linked files ({present} already present)",
        pending.len()
    );
    // Many links point at slow or dead hosts; download in parallel with a short timeout.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let outcomes = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..LINK_WORKERS {
            scope.spawn(|| {
                while let Some((id, url, pdf, md5)) =
                    pending.get(next.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
                {
                    let outcome = download(url, pdf).and_then(|()| {
                        let actual = format!("{:x}", Md5::digest(fs::read(pdf)?));
                        if actual == *md5 {
                            Ok(())
                        } else {
                            bail!("MD5 mismatch from {url}")
                        }
                    });
                    if outcome.is_err() {
                        let _ = fs::remove_file(pdf);
                    }
                    if let Ok(mut outcomes) = outcomes.lock() {
                        outcomes.push(outcome.map_err(|error| format!("{id}: {error:#}")));
                    }
                }
            });
        }
    });
    let outcomes = outcomes.into_inner().unwrap_or_default();
    let fetched = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    failed.extend(outcomes.into_iter().filter_map(Result::err));
    failed.sort();
    println!(
        "linked files: {fetched} downloaded, {present} already present, {} unavailable",
        failed.len()
    );
    for failure in &failed {
        println!("  {failure}");
    }
    Ok(())
}

/// Downloads `url` to `destination` with curl.
pub fn download(url: &str, destination: &Path) -> Result<()> {
    run_command(
        Command::new("curl")
            .args([
                "-sSfL",
                "--retry",
                "1",
                "--connect-timeout",
                "15",
                "--max-time",
                "90",
                "-o",
            ])
            .arg(destination)
            .arg(url),
    )
    .with_context(|| format!("downloading {url}"))
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()?;
    if !output.status.success() {
        bail!("git {} failed in {}", args.join(" "), root.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Returns the checked-out commit of a corpus, if it is a git checkout.
pub fn revision(root: &Path) -> Option<String> {
    root.join(".git")
        .exists()
        .then(|| git_output(root, &["rev-parse", "HEAD"]).ok())
        .flatten()
}

fn run_command(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("launching {command:?}"))?;
    if !status.success() {
        bail!("{command:?} exited with {status}");
    }
    Ok(())
}
