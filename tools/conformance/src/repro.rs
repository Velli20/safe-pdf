//! Reproduces an issue locally without a corpus checkout or a PDFium build.
//!
//! The published report on GitHub Pages holds each failing page's PDFium image
//! (`cases/<dir>/pN-ref.png`) and the run index. `repro` looks the issue key (or a case id)
//! up in that index, checks out only the PDFs it needs at the published corpus revision,
//! downloads their reference images, and reruns those cases against them.

use crate::{
    corpus::{self, CorpusKind},
    fetch, issue_body, issue_state,
    report_index::Index,
    run::{self, RunOptions},
};
use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// Arguments of `repro` and `verify`.
#[derive(clap::Args)]
pub struct ReproArgs {
    /// Issue key (`conf2-…`) or a case id.
    pub target: String,
    /// Corpora to look in (default: all).
    #[arg(long, value_enum)]
    pub corpus: Vec<CorpusKind>,
    /// Cases rerun at most per corpus.
    #[arg(long, default_value_t = 10)]
    pub max_cases: usize,
    /// Repository whose GitHub Pages report is used, as owner/name (default: from `origin`).
    #[arg(long, env = "GITHUB_REPOSITORY")]
    pub repo: Option<String>,
    /// Parallel workers.
    #[arg(short, long, default_value_t = 4)]
    pub jobs: usize,
}

/// What the target selects in one corpus.
struct Selection {
    kind: CorpusKind,
    index: Index,
    /// Signatures whose key is the target; empty when the target is a case id.
    signatures: BTreeSet<String>,
    cases: Vec<String>,
}

/// Reruns the target's cases and returns true when none still fails with its signature (or,
/// for a case id, when the case no longer fails).
pub fn run(args: &ReproArgs) -> Result<bool> {
    let repo = match &args.repo {
        Some(repo) => repo.clone(),
        None => origin_repo()?,
    };
    let kinds = if args.corpus.is_empty() {
        vec![CorpusKind::Pdfium, CorpusKind::Pdfjs]
    } else {
        args.corpus.clone()
    };
    let mut selections = Vec::new();
    for kind in kinds {
        let base = issue_body::pages_url(&repo, kind);
        let dir = work_dir(kind);
        fs::create_dir_all(&dir)?;
        let path = dir.join("published-index.json");
        if let Err(error) = fetch::download(&format!("{base}index.json"), &path) {
            println!(
                "{}: no published report at {base} ({error:#})",
                kind.as_str()
            );
            continue;
        }
        let index: Index = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("reading {}", path.display()))?;
        if let Some(selection) = select(kind, index, &args.target, args.max_cases) {
            selections.push(selection);
        }
    }
    if selections.is_empty() {
        bail!(
            "`{}` matches no failure cluster or case in the published reports of {repo}",
            args.target
        );
    }
    let mut fixed = true;
    for selection in &selections {
        fixed &= rerun(args, &repo, selection)?;
    }
    println!(
        "\n{}",
        if fixed {
            "No selected case fails this way any more."
        } else {
            "Some cases still fail this way."
        }
    );
    Ok(fixed)
}

fn work_dir(kind: CorpusKind) -> PathBuf {
    corpus::state_dir().join("repro").join(kind.as_str())
}

fn select(kind: CorpusKind, index: Index, target: &str, limit: usize) -> Option<Selection> {
    let clusters: Vec<_> = index
        .clusters
        .iter()
        .filter(|cluster| issue_state::key(&cluster.signature) == target)
        .collect();
    let (signatures, cases): (BTreeSet<String>, Vec<String>) = if clusters.is_empty() {
        let case = index.cases.iter().find(|case| case.id == target)?;
        (BTreeSet::new(), vec![case.id.clone()])
    } else {
        let cases: BTreeSet<String> = clusters
            .iter()
            .flat_map(|cluster| cluster.cases.iter().cloned())
            .collect();
        (
            clusters
                .iter()
                .map(|cluster| cluster.signature.clone())
                .collect(),
            cases.into_iter().take(limit).collect(),
        )
    };
    Some(Selection {
        kind,
        index,
        signatures,
        cases,
    })
}

fn rerun(args: &ReproArgs, repo: &str, selection: &Selection) -> Result<bool> {
    let kind = selection.kind;
    println!(
        "{}: rerunning {} against the published PDFium images",
        kind.as_str(),
        selection.cases.join(", ")
    );
    let dir = work_dir(kind);
    let root = dir.join("corpus");
    let (_, pinned) = kind.source();
    let revision = selection.index.corpus_revision.as_deref().unwrap_or(pinned);
    checkout(kind, &root, revision, &selection.cases)?;
    let references = dir.join("references");
    let base = issue_body::pages_url(repo, kind);
    for id in &selection.cases {
        let Some(case) = selection.index.cases.iter().find(|case| &case.id == id) else {
            continue;
        };
        let case_dir = references.join(&case.dir);
        fs::create_dir_all(&case_dir)?;
        for file in case
            .pages
            .iter()
            .flat_map(|page| &page.files)
            .filter(|file| file.ends_with("-ref.png"))
        {
            let path = case_dir.join(file);
            if !path.is_file() {
                fetch::download(&format!("{base}cases/{}/{file}", case.dir), &path)?;
            }
        }
    }
    let options = RunOptions {
        kind,
        root,
        explicit_root: true,
        filter: None,
        cases: selection.cases.clone(),
        page: None,
        jobs: args.jobs,
        scale: selection.index.scale,
        max_side: 3000,
        max_pages: 10,
        timeout: Duration::from_secs(60),
        tolerance: selection.index.tolerance,
        pdfium: None,
        reference_images: Some(references),
    };
    run::run(&options)?;
    let index: Index =
        serde_json::from_slice(&fs::read(corpus::output_dir(kind).join("index.json"))?)?;
    let mut fixed = true;
    for case in &index.cases {
        let signatures: Vec<&String> = case
            .pages
            .iter()
            .filter(|page| page.status.is_failure())
            .filter_map(|page| page.signature.as_ref())
            .chain(case.signature.as_ref())
            .collect();
        let still = if selection.signatures.is_empty() {
            case.status.is_failure()
        } else {
            signatures
                .iter()
                .any(|signature| selection.signatures.contains(*signature))
        };
        fixed &= !still;
        println!(
            "  {} {}: {}{}",
            if still { "✗" } else { "✓" },
            case.id,
            case.status.as_str(),
            case.signature
                .as_ref()
                .map(|signature| format!(" ({signature})"))
                .unwrap_or_default()
        );
    }
    Ok(fixed)
}

/// Checks out the selected cases' files; pdf.js cases also need the manifest and may be
/// linked files downloaded from their original URLs.
fn checkout(kind: CorpusKind, root: &Path, revision: &str, cases: &[String]) -> Result<()> {
    match kind {
        CorpusKind::Pdfium => fetch::sparse_checkout(kind, root, revision, cases),
        CorpusKind::Pdfjs => {
            fetch::sparse_checkout(
                kind,
                root,
                revision,
                &["test/test_manifest.json".to_owned()],
            )?;
            let ids: BTreeSet<String> = cases.iter().cloned().collect();
            let manifest = crate::corpus_pdfjs::manifest(root)?;
            let paths: Vec<String> = manifest
                .iter()
                .filter(|entry| ids.contains(&entry.id))
                .map(|entry| {
                    format!(
                        "test/{}{}",
                        entry.file,
                        if entry.link { ".link" } else { "" }
                    )
                })
                .collect();
            fetch::sparse_checkout(kind, root, revision, &paths)?;
            if manifest
                .iter()
                .any(|entry| entry.link && ids.contains(&entry.id))
            {
                fetch::fetch_links(root, Some(&ids))?;
            }
            Ok(())
        }
    }
}

/// Returns `owner/name` of the workspace's `origin` remote.
fn origin_repo() -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(corpus::workspace_root())
        .args(["remote", "get-url", "origin"])
        .output()?;
    let url = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    repo_from_url(&url).with_context(|| {
        format!("cannot tell the repository from origin `{url}`; pass --repo owner/name")
    })
}

fn repo_from_url(url: &str) -> Option<String> {
    let path = url.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = path.rsplit(['/', ':']);
    let name = parts.next().filter(|part| !part.is_empty())?;
    let owner = parts.next().filter(|part| !part.is_empty())?;
    Some(format!("{owner}/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_repository_from_remotes() {
        for url in [
            "https://github.com/Velli20/safe-pdf.git",
            "git@github.com:Velli20/safe-pdf.git",
            "http://proxy@127.0.0.1:8080/git/Velli20/safe-pdf",
        ] {
            assert_eq!(repo_from_url(url).as_deref(), Some("Velli20/safe-pdf"));
        }
        assert_eq!(repo_from_url(""), None);
    }
}
