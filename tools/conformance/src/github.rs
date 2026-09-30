//! GitHub access through the `gh` command-line tool.

use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use std::process::Command;

/// An issue with the fields the conformance workflow reads.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// Issue number.
    pub number: u64,
    /// Title.
    #[serde(default)]
    pub title: String,
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
    let output = Command::new("gh")
        .args(args)
        .output()
        .map_err(|error| anyhow!("cannot run gh ({error}); install the GitHub CLI"))?;
    if !output.status.success() {
        bail!(
            "gh {} failed: {}",
            args.iter().take(2).copied().collect::<Vec<_>>().join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Lists every issue with `label`, open and closed.
pub fn issues(repo: &str, label: &str) -> Result<Vec<Issue>> {
    let json = gh(&[
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
        "number,title,state,stateReason,body",
    ])?;
    Ok(serde_json::from_str(&json)?)
}
