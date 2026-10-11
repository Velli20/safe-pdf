//! Machine-readable state kept in a hidden comment at the top of each conformance issue.
//!
//! The state carries the issue's key and, per corpus, the size of its cluster in the last
//! run and how many full runs in a row it was absent. Issues without it are not matched.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MARKER: &str = "<!-- conformance-state: ";
const MARKER_END: &str = " -->";
/// Version of the issue body layout. Raising it rewrites every open issue on the next run.
/// 1: images link to the `conformance-assets` branch instead of the Pages report.
pub const FORMAT: u32 = 1;
/// Prefix of issue keys.
pub const KEY_PREFIX: &str = "conf2-";

/// Cluster size of an issue in one corpus.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusState {
    /// Failing pages (or documents without pages) in the last run that had the cluster.
    pub failures: usize,
    /// Documents in the cluster in that run.
    pub documents: usize,
    /// Consecutive full runs of this corpus without the cluster.
    #[serde(default)]
    pub missing: u32,
}

/// State of one conformance issue.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueState {
    /// Cluster key.
    pub key: String,
    /// Cluster size per corpus name.
    #[serde(default)]
    pub corpora: BTreeMap<String, CorpusState>,
    /// Body layout the issue was last written with; see [`FORMAT`].
    #[serde(default)]
    pub format: u32,
}

/// Returns the key of a signature. Keys do not depend on the corpus, so one cause found in
/// several corpora is one issue.
pub fn key(signature: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(signature.as_bytes()));
    format!("{KEY_PREFIX}{}", digest.get(..12).unwrap_or_default())
}

impl IssueState {
    /// Reads the state of an issue body.
    pub fn parse(body: &str) -> Option<Self> {
        let (_, rest) = body.split_once(MARKER)?;
        let (json, _) = rest.split_once(MARKER_END)?;
        serde_json::from_str(json).ok()
    }

    /// Returns the hidden comment holding this state.
    pub fn marker(&self) -> Result<String> {
        Ok(format!(
            "{MARKER}{}{MARKER_END}",
            serde_json::to_string(self)?
        ))
    }

    /// Returns `body` with its state comment replaced by this state, or prepended to it.
    pub fn apply(&self, body: &str) -> Result<String> {
        let marker = self.marker()?;
        if let Some((head, rest)) = body.split_once(MARKER)
            && let Some((_, tail)) = rest.split_once(MARKER_END)
        {
            return Ok(format!("{head}{marker}{tail}"));
        }
        Ok(format!("{marker}\n{body}"))
    }

    /// Total failures across corpora.
    pub fn failures(&self) -> usize {
        self.corpora
            .values()
            .fold(0, |sum, corpus| sum.saturating_add(corpus.failures))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_body() {
        let mut state = IssueState {
            key: key("mismatch: blank page"),
            corpora: BTreeMap::new(),
            format: FORMAT,
        };
        state.corpora.insert(
            "pdfium".to_owned(),
            CorpusState {
                failures: 4,
                documents: 2,
                missing: 1,
            },
        );
        let body = state.apply("Body text").unwrap();
        assert!(body.ends_with("\nBody text"));
        assert_eq!(IssueState::parse(&body), Some(state.clone()));
        state.corpora.clear();
        let replaced = state.apply(&body).unwrap();
        assert_eq!(IssueState::parse(&replaced), Some(state));
        assert!(replaced.ends_with("\nBody text"));
        assert_eq!(replaced.matches(MARKER).count(), 1);
    }

    #[test]
    fn keys_are_short_and_prefixed() {
        assert!(key("x").starts_with(KEY_PREFIX));
        assert_eq!(key("x").len(), KEY_PREFIX.len() + 12);
        assert_eq!(
            IssueState::parse("Conformance key: `conf-1b1118ae25c9`."),
            None
        );
    }
}
