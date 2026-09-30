//! Pull request report: what a change fixes and breaks, compared with open issues and the
//! baselines. It only reports; it never fails the pull request.

use crate::{
    corpus::CorpusKind,
    github,
    issue_state::{self, IssueState},
    issues::{self, CorpusRun},
};
use anyhow::Result;
use std::{collections::BTreeSet, fmt::Write as _, fs, path::Path};

/// Hidden marker identifying the report comment.
pub const MARKER: &str = "<!-- conformance-pr-report -->";
/// Lines listed per section.
const LIST_LIMIT: usize = 25;

/// Writes the report for the last runs of `kinds` to `out`.
pub fn run(kinds: &[CorpusKind], repo: &str, out: &Path) -> Result<()> {
    let runs = kinds
        .iter()
        .map(|kind| CorpusRun::load(*kind, false))
        .collect::<Result<Vec<_>>>()?;
    let issues = github::issues(repo, "conformance").unwrap_or_else(|error| {
        println!("cannot list issues, so fixed issues are not reported: {error:#}");
        Vec::new()
    });
    let states: Vec<(github::Issue, IssueState)> = issues
        .into_iter()
        .filter_map(|issue| IssueState::parse(&issue.title, &issue.body).map(|s| (issue, s)))
        .collect();
    let groups = issues::groups(&runs);
    let current: BTreeSet<&str> = groups.iter().map(|group| group.key.as_str()).collect();
    let filtered = runs.iter().any(|run| run.index.filtered);

    let mut text = format!("{MARKER}\n## Conformance report\n\n");
    for run in &runs {
        writeln!(
            text,
            "- **{}**: {}",
            run.kind.as_str(),
            run.index.totals_line()
        )?;
    }
    writeln!(text)?;

    let regressions: Vec<String> = runs
        .iter()
        .flat_map(|run| {
            run.index
                .regressions()
                .into_iter()
                .map(move |line| format!("{}: {line}", run.kind.as_str()))
        })
        .collect();
    writeln!(text, "### Regressions against the baseline\n")?;
    list(&mut text, &regressions, "None.")?;

    let fixed: Vec<String> = if filtered {
        Vec::new()
    } else {
        states
            .iter()
            .filter(|(issue, state)| {
                issue.is_open()
                    && !state.is_legacy()
                    && !current.contains(state.key.as_str())
                    && runs.iter().any(|run| {
                        state
                            .corpora
                            .get(run.kind.as_str())
                            .is_some_and(|entry| entry.failures > 0 && entry.missing == 0)
                    })
            })
            .map(|(issue, _)| format!("#{} {}", issue.number, issue.title))
            .collect()
    };
    writeln!(text, "### Issues that no longer reproduce\n")?;
    list(
        &mut text,
        &fixed,
        if filtered {
            "Not checked: the run covered only part of a corpus."
        } else {
            "None."
        },
    )?;

    let known: BTreeSet<&str> = states.iter().map(|(_, state)| state.key.as_str()).collect();
    let new: Vec<String> = groups
        .iter()
        .filter(|group| {
            !known.contains(group.key.as_str())
                && group
                    .legacy_keys()
                    .iter()
                    .all(|key| !known.contains(key.as_str()))
        })
        .map(|group| {
            format!(
                "`{}`: {} failures in {} documents",
                group.signature,
                group.failures(),
                group.documents()
            )
        })
        .collect();
    writeln!(text, "### New failure clusters\n")?;
    list(&mut text, &new, "None.")?;

    writeln!(
        text,
        "<sub>Report only: this check never fails the pull request. Issue keys use `{}`; see the \
         uploaded `conformance-<corpus>` artifacts for images.</sub>",
        issue_state::KEY_PREFIX
    )?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, &text)?;
    print!("{text}");
    Ok(())
}

fn list(text: &mut String, lines: &[String], empty: &str) -> Result<()> {
    if lines.is_empty() {
        writeln!(text, "{empty}\n")?;
        return Ok(());
    }
    for line in lines.iter().take(LIST_LIMIT) {
        writeln!(text, "- {line}")?;
    }
    if lines.len() > LIST_LIMIT {
        writeln!(
            text,
            "- …and {} more",
            lines.len().saturating_sub(LIST_LIMIT)
        )?;
    }
    writeln!(text)?;
    Ok(())
}
