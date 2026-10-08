//! GitHub access through the `gh` command-line tool.

use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::{process::Command, thread, time::Duration};

/// Attempts of a call that can safely be repeated.
const ATTEMPTS: u32 = 3;
/// Wait before the first repeat; doubled before each further one.
const FIRST_RETRY_DELAY: Duration = Duration::from_secs(5);
/// `gh` errors from GitHub's side that a repeat of the same call can get past.
const TRANSIENT_ERRORS: &[&str] = &[
    "Something went wrong while executing your query",
    "HTTP 500",
    "HTTP 502",
    "HTTP 503",
    "HTTP 504",
    "timeout",
    "connection reset",
];

/// An issue with the fields the conformance workflow reads.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// Issue number.
    pub number: u64,
    /// `OPEN` or `CLOSED`.
    pub state: String,
    /// `COMPLETED`, `NOT_PLANNED` or `REOPENED` when set.
    #[serde(default)]
    pub state_reason: Option<String>,
    /// Markdown body.
    #[serde(default)]
    pub body: String,
}

impl Issue {
    /// Returns true when the issue is open.
    pub fn is_open(&self) -> bool {
        self.state == "OPEN"
    }

    /// Returns true when the issue was closed as not planned, which the workflow respects.
    pub fn is_not_planned(&self) -> bool {
        !self.is_open() && self.state_reason.as_deref() == Some("NOT_PLANNED")
    }
}

/// Runs `gh` and returns its standard output.
pub fn gh(args: &[&str]) -> Result<String> {
    run(args).map_err(|failure| failure.error)
}

/// Runs `gh` like [`gh`], repeating the call when GitHub fails it with a server-side error.
///
/// Only for calls that leave the same result when applied twice, such as listing issues,
/// setting an issue's title, body and labels, or `label create --force`: a failed call may
/// still have been applied.
pub fn gh_idempotent(args: &[&str]) -> Result<String> {
    let mut delay = FIRST_RETRY_DELAY;
    let mut attempt = 1;
    loop {
        match run(args) {
            Ok(output) => return Ok(output),
            Err(failure) if failure.transient && attempt < ATTEMPTS => {
                println!("{:#}; retrying in {}s", failure.error, delay.as_secs());
                thread::sleep(delay);
                delay = delay.saturating_mul(2);
                attempt = attempt.saturating_add(1);
            }
            Err(failure) => return Err(failure.error),
        }
    }
}

/// A failed `gh` call.
struct Failure {
    error: anyhow::Error,
    /// The error came from GitHub's side and may not recur.
    transient: bool,
}

fn run(args: &[&str]) -> std::result::Result<String, Failure> {
    let output = Command::new("gh")
        .args(args)
        .output()
        .map_err(|error| Failure {
            error: anyhow!("cannot run gh ({error}); install the GitHub CLI"),
            transient: false,
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Failure {
            error: anyhow!(
                "gh {} failed: {}",
                args.iter().take(2).copied().collect::<Vec<_>>().join(" "),
                stderr.trim()
            ),
            transient: is_transient(&stderr),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Returns true when `gh`'s error output names a server-side failure worth repeating.
fn is_transient(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    TRANSIENT_ERRORS
        .iter()
        .any(|error| stderr.contains(&error.to_ascii_lowercase()))
}

/// Lists every issue with `label`, open and closed.
pub fn issues(repo: &str, label: &str) -> Result<Vec<Issue>> {
    let json = gh_idempotent(&[
        "issue",
        "list",
        "--repo",
        repo,
        "--label",
        label,
        "--state",
        "all",
        "--limit",
        "5000",
        "--json",
        "number,state,stateReason,body",
    ])?;
    Ok(serde_json::from_str(&json)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphql_server_errors_are_transient() {
        assert!(is_transient(
            "failed to update https://github.com/o/r/issues/502: GraphQL: Something went wrong \
             while executing your query on 2026-10-08T15:42:30Z."
        ));
        assert!(is_transient(
            "HTTP 502: Bad Gateway (https://api.github.com/graphql)"
        ));
    }

    #[test]
    fn request_errors_are_not_transient() {
        assert!(!is_transient(
            "GraphQL: Could not resolve to an issue (repository.issue)"
        ));
        assert!(!is_transient("HTTP 422: Validation Failed"));
    }
}
