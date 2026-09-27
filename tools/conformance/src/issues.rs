//! Files one GitHub issue per failure cluster, skipping clusters already reported.
//!
//! Each issue carries a key derived from the corpus and cluster signature. Existing
//! issues are found through the issue list API (not search, which indexes with a delay):
//! - no issue: create one (up to `max_new` per run, regressions first);
//! - open issue: refresh its body and comment when the cluster size changed;
//! - closed as completed: reopen, since the failure is back;
//! - closed as not planned: leave it alone.

use crate::{
    baseline::Delta,
    corpus::{self, CorpusKind},
    model::Status,
    report_index::{Cluster, Index},
    run,
};
use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, fs, path::Path, process::Command};

/// GitHub rejects issue bodies above 65,536 characters.
const BODY_LIMIT: usize = 60_000;
/// Cases listed per issue.
const CASE_LIMIT: usize = 15;
const LABEL: &str = "conformance";

/// Settings of an issue run.
pub struct IssueOptions<'a> {
    /// Corpus whose last run is reported.
    pub kind: CorpusKind,
    /// `owner/name` of the repository.
    pub repo: &'a str,
    /// Issues created at most per run.
    pub max_new: usize,
    /// Print planned actions and write bodies to disk without calling GitHub.
    pub dry_run: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Issue {
    number: u64,
    state: String,
    #[serde(default)]
    state_reason: Option<String>,
    body: String,
}

/// Files issues for the clusters of the corpus's last run.
pub fn run(options: &IssueOptions<'_>) -> Result<()> {
    let out = corpus::output_dir(options.kind);
    let index: Index = serde_json::from_slice(
        &fs::read(out.join("index.json")).context("no index.json; run the corpus first")?,
    )?;
    let existing = if options.dry_run {
        existing_issues(options.repo).unwrap_or_else(|error| {
            println!("(dry run) cannot list issues, treating every cluster as new: {error:#}");
            Vec::new()
        })
    } else {
        ensure_labels(options)?;
        existing_issues(options.repo)?
    };
    let run_link = run_link();
    let mut clusters: Vec<&Cluster> = index.clusters.iter().collect();
    clusters.sort_by_key(|cluster| {
        (
            !regressed(&index, cluster),
            std::cmp::Reverse(cluster.count),
        )
    });
    let (mut created, mut updated, mut reopened, mut skipped, mut failed) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for cluster in clusters {
        let key = key(options.kind, &cluster.signature);
        let body = body(
            options.kind,
            &index,
            cluster,
            &key,
            &out,
            run_link.as_deref(),
        )?;
        let issue = existing.iter().find(|issue| issue.body.contains(&key));
        let result = match issue {
            None if created >= options.max_new => {
                skipped = skipped.saturating_add(1);
                continue;
            }
            None => {
                created = created.saturating_add(1);
                create(options, &index, cluster, &key, &body, &out)
            }
            Some(issue) if issue.state == "OPEN" => {
                if count_marker(&issue.body) == Some(cluster.count) {
                    continue;
                }
                updated = updated.saturating_add(1);
                refresh(options, issue, cluster, &body, run_link.as_deref())
            }
            Some(issue) if issue.state_reason.as_deref() == Some("NOT_PLANNED") => continue,
            Some(issue) => {
                reopened = reopened.saturating_add(1);
                reopen(options, issue, &body, run_link.as_deref())
            }
        };
        if let Err(error) = result {
            failed = failed.saturating_add(1);
            println!("failed for `{}`: {error:#}", cluster.signature);
        }
    }
    println!(
        "{} clusters: {created} created, {updated} updated, {reopened} reopened, {skipped} deferred by --max-new, {failed} failed",
        index.clusters.len()
    );
    if failed > 0 {
        bail!("{failed} issue operations failed");
    }
    Ok(())
}

/// Returns the stable issue key of a cluster.
fn key(kind: CorpusKind, signature: &str) -> String {
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{}\n{signature}", kind.as_str()))
    );
    format!("conf-{}", digest.get(..12).unwrap_or_default())
}

fn count_marker(body: &str) -> Option<usize> {
    let rest = body.split("<!-- conformance-count: ").nth(1)?;
    rest.split(' ').next()?.parse().ok()
}

/// Returns true when any failure of the cluster regressed against the baseline or is a new crash.
fn regressed(index: &Index, cluster: &Cluster) -> bool {
    let bad = |delta: Delta, status: Status| {
        delta == Delta::Regressed
            || (delta == Delta::New && matches!(status, Status::Crash | Status::Timeout))
    };
    index
        .cases
        .iter()
        .filter(|case| cluster.cases.contains(&case.id))
        .any(|case| {
            (case.signature.as_ref() == Some(&cluster.signature) && bad(case.delta, case.status))
                || case.pages.iter().any(|page| {
                    page.signature.as_ref() == Some(&cluster.signature)
                        && bad(page.delta, page.status)
                })
        })
}

fn run_link() -> Option<String> {
    let server = std::env::var("GITHUB_SERVER_URL").ok()?;
    let repo = std::env::var("GITHUB_REPOSITORY").ok()?;
    let id = std::env::var("GITHUB_RUN_ID").ok()?;
    Some(format!("{server}/{repo}/actions/runs/{id}"))
}

fn title(kind: CorpusKind, signature: &str) -> String {
    let mut title = format!("[conformance/{}] {signature}", kind.as_str());
    if title.chars().count() > 200 {
        title = title.chars().take(199).collect();
        title.push('…');
    }
    title
}

fn labels(kind: CorpusKind, signature: &str, regressed: bool) -> Vec<String> {
    let mut labels = vec![LABEL.to_owned(), format!("{LABEL}:{}", kind.as_str())];
    if signature.starts_with("crash") || signature.starts_with("timeout") {
        labels.push("crash".to_owned());
    }
    if regressed {
        labels.push("regression".to_owned());
    }
    labels
}

fn body(
    kind: CorpusKind,
    index: &Index,
    cluster: &Cluster,
    key: &str,
    out: &Path,
    run_link: Option<&str>,
) -> Result<String> {
    let mut text = String::new();
    writeln!(
        text,
        "<!-- conformance-count: {} -->\nAutomatically filed by the conformance workflow for the **{}** corpus \
         (reference: {}).\n",
        cluster.count,
        kind.as_str(),
        index.pdfium
    )?;
    writeln!(
        text,
        "**{} failures in {} documents** share the signature\n\n```\n{}\n```\n",
        cluster.count,
        cluster.cases.len(),
        cluster.signature
    )?;
    if let Some(link) = run_link {
        writeln!(
            text,
            "Latest run: {link}. Download the `conformance-{}` artifact for images, the HTML viewer and `TRIAGE.md`.\n",
            kind.as_str()
        )?;
    }
    if !cluster.likely_crates.is_empty() {
        let crates: Vec<String> = cluster
            .likely_crates
            .iter()
            .take(6)
            .map(|(name, count)| format!("{name} ({count})"))
            .collect();
        writeln!(text, "Features point at: {}\n", crates.join(", "))?;
    }
    writeln!(text, "### Cases\n")?;
    for id in cluster.cases.iter().take(CASE_LIMIT) {
        let Some(case) = index.cases.iter().find(|case| &case.id == id) else {
            continue;
        };
        let worst = case
            .pages
            .iter()
            .filter(|page| page.signature.as_ref() == Some(&cluster.signature))
            .filter_map(|page| page.mismatch)
            .fold(None, |max: Option<f64>, value| {
                Some(max.map_or(value, |m| m.max(value)))
            });
        writeln!(
            text,
            "- `{}`{}: `{}`",
            case.id,
            worst
                .map(|value| format!(" (mismatch {:.2}%)", value * 100.0))
                .unwrap_or_default(),
            case.reproduce
        )?;
    }
    if cluster.cases.len() > CASE_LIMIT {
        writeln!(
            text,
            "- … {} more",
            cluster.cases.len().saturating_sub(CASE_LIMIT)
        )?;
    }
    let footer = format!(
        "\n---\nConformance key: `{key}`. Closing this issue as *not planned* stops the workflow from reopening it.\n"
    );
    if let Some(first) = cluster.cases.first() {
        let summary = out
            .join("cases")
            .join(run::case_dir(first))
            .join("summary.md");
        if let Ok(summary) = fs::read_to_string(summary) {
            writeln!(
                text,
                "\n<details><summary>Summary of <code>{first}</code></summary>\n"
            )?;
            let budget = BODY_LIMIT
                .saturating_sub(text.len())
                .saturating_sub(footer.len())
                .saturating_sub(64);
            let mut excerpt: String = summary.chars().take(budget).collect();
            if excerpt.len() < summary.len() {
                excerpt.push_str("\n\n… (truncated; full summary in the artifact)");
            }
            writeln!(text, "{excerpt}\n\n</details>")?;
        }
    }
    text.push_str(&footer);
    Ok(text)
}

fn gh(args: &[&str]) -> Result<String> {
    let output = Command::new("gh")
        .args(args)
        .output()
        .map_err(|error| anyhow!("cannot run gh ({error}); install the GitHub CLI"))?;
    if !output.status.success() {
        bail!(
            "gh {} failed: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn existing_issues(repo: &str) -> Result<Vec<Issue>> {
    let json = gh(&[
        "issue",
        "list",
        "--repo",
        repo,
        "--label",
        LABEL,
        "--state",
        "all",
        "--limit",
        "5000",
        "--json",
        "number,state,stateReason,body",
    ])?;
    Ok(serde_json::from_str(&json)?)
}

fn ensure_labels(options: &IssueOptions<'_>) -> Result<()> {
    let corpus_label = format!("{LABEL}:{}", options.kind.as_str());
    for (name, color, description) in [
        (LABEL, "5319e7", "Filed by the conformance workflow"),
        (corpus_label.as_str(), "c5def5", "Conformance corpus"),
        ("crash", "b60205", "Panic, abort or timeout"),
        ("regression", "d93f0b", "Worse than the recorded baseline"),
    ] {
        gh(&[
            "label",
            "create",
            name,
            "--repo",
            options.repo,
            "--color",
            color,
            "--description",
            description,
            "--force",
        ])?;
    }
    Ok(())
}

/// Writes a body to a temporary file for `--body-file`; bodies exceed argument limits.
fn body_file(out: &Path, key: &str, body: &str) -> Result<std::path::PathBuf> {
    let dir = out.join("issues");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{key}.md"));
    fs::write(&path, body)?;
    Ok(path)
}

fn create(
    options: &IssueOptions<'_>,
    index: &Index,
    cluster: &Cluster,
    key: &str,
    body: &str,
    out: &Path,
) -> Result<()> {
    let title = title(options.kind, &cluster.signature);
    let labels = labels(options.kind, &cluster.signature, regressed(index, cluster));
    let file = body_file(out, key, body)?;
    if options.dry_run {
        println!(
            "(dry run) create `{title}` [{}] body {} ({} chars)",
            labels.join(", "),
            file.display(),
            body.len()
        );
        return Ok(());
    }
    let file = file.display().to_string();
    let mut args = vec![
        "issue",
        "create",
        "--repo",
        options.repo,
        "--title",
        &title,
        "--body-file",
        &file,
    ];
    for label in &labels {
        args.extend(["--label", label.as_str()]);
    }
    let url = gh(&args)?;
    println!("created {} for `{}`", url.trim(), cluster.signature);
    Ok(())
}

fn refresh(
    options: &IssueOptions<'_>,
    issue: &Issue,
    cluster: &Cluster,
    body: &str,
    run_link: Option<&str>,
) -> Result<()> {
    let previous =
        count_marker(&issue.body).map_or_else(|| "unknown".to_owned(), |n| n.to_string());
    let note = format!(
        "Cluster size changed: {} failures in {} documents (was {previous}){}.",
        cluster.count,
        cluster.cases.len(),
        run_link
            .map(|link| format!(" in {link}"))
            .unwrap_or_default()
    );
    let number = issue.number.to_string();
    if options.dry_run {
        println!("(dry run) update #{number}: {note}");
        return Ok(());
    }
    let file = body_file(&corpus::output_dir(options.kind), &number, body)?
        .display()
        .to_string();
    gh(&[
        "issue",
        "edit",
        &number,
        "--repo",
        options.repo,
        "--body-file",
        &file,
    ])?;
    gh(&[
        "issue",
        "comment",
        &number,
        "--repo",
        options.repo,
        "--body",
        &note,
    ])?;
    println!("updated #{number} for `{}`", cluster.signature);
    Ok(())
}

fn reopen(
    options: &IssueOptions<'_>,
    issue: &Issue,
    body: &str,
    run_link: Option<&str>,
) -> Result<()> {
    let number = issue.number.to_string();
    let note = format!(
        "This failure reappeared{}. Reopening.",
        run_link
            .map(|link| format!(" in {link}"))
            .unwrap_or_default()
    );
    if options.dry_run {
        println!("(dry run) reopen #{number}");
        return Ok(());
    }
    let file = body_file(&corpus::output_dir(options.kind), &number, body)?
        .display()
        .to_string();
    gh(&[
        "issue",
        "edit",
        &number,
        "--repo",
        options.repo,
        "--body-file",
        &file,
    ])?;
    gh(&[
        "issue",
        "reopen",
        &number,
        "--repo",
        options.repo,
        "--comment",
        &note,
    ])?;
    gh(&[
        "issue",
        "edit",
        &number,
        "--repo",
        options.repo,
        "--add-label",
        "regression",
    ])?;
    println!("reopened #{number}");
    Ok(())
}
