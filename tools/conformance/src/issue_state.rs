//! Machine-readable state kept in a hidden comment at the top of each conformance issue.
//!
//! The state carries the issue's key and, per corpus, the size of its cluster in the last
//! run and how many full runs in a row it was absent. Issues filed before this state existed
//! carry only a `Conformance key` footer and a count marker; they are read as legacy state.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MARKER: &str = "<!-- conformance-state: ";
const MARKER_END: &str = " -->";
/// Prefix of keys derived from root-cause signatures; older keys start with `conf-`.
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
}

/// Returns the key of a signature. Keys do not depend on the corpus, so one cause found in
/// several corpora is one issue.
pub fn key(signature: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(signature.as_bytes()));
    format!("{KEY_PREFIX}{}", digest.get(..12).unwrap_or_default())
}

/// Returns the key issues were filed under before root-cause signatures.
pub fn legacy_key(corpus: &str, signature: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(format!("{corpus}\n{signature}")));
    format!("conf-{}", digest.get(..12).unwrap_or_default())
}

impl IssueState {
    /// Reads the state of an issue body, falling back to the legacy footer and count marker.
    pub fn parse(title: &str, body: &str) -> Option<Self> {
        if let Some(json) = body
            .split_once(MARKER)
            .and_then(|(_, rest)| rest.split_once(MARKER_END))
            .map(|(json, _)| json)
        {
            return serde_json::from_str(json).ok();
        }
        let key = body
            .split_once("Conformance key: `")
            .and_then(|(_, rest)| rest.split_once('`'))
            .map(|(key, _)| key.to_owned())?;
        let mut state = Self {
            key,
            corpora: BTreeMap::new(),
        };
        if let Some(corpus) = title
            .strip_prefix("[conformance/")
            .and_then(|rest| rest.split_once(']'))
            .map(|(corpus, _)| corpus.to_owned())
        {
            state.corpora.insert(
                corpus,
                CorpusState {
                    failures: legacy_count(body).unwrap_or(0),
                    documents: legacy_documents(body).unwrap_or(0),
                    missing: 0,
                },
            );
        }
        Some(state)
    }

    /// Returns true when the key predates root-cause signatures.
    pub fn is_legacy(&self) -> bool {
        !self.key.starts_with(KEY_PREFIX)
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

/// Returns the signature quoted in a legacy issue body (its first fenced block).
pub fn legacy_signature(body: &str) -> Option<&str> {
    body.split_once("```\n")
        .and_then(|(_, rest)| rest.split_once("\n```"))
        .map(|(signature, _)| signature)
}

fn legacy_count(body: &str) -> Option<usize> {
    let rest = body.split("<!-- conformance-count: ").nth(1)?;
    rest.split(' ').next()?.parse().ok()
}

fn legacy_documents(body: &str) -> Option<usize> {
    let rest = body.split(" failures in ").nth(1)?;
    rest.split(' ').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: &str = "<!-- conformance-count: 3 -->\nAutomatically filed.\n\n**3 failures in 2 documents** share the signature\n\n```\nrender_error: boom @ x\n```\n\n---\nConformance key: `conf-1b1118ae25c9`. Closing…\n";

    #[test]
    fn reads_legacy_issues() {
        let state = IssueState::parse("[conformance/pdfjs] render_error: boom", LEGACY).unwrap();
        assert_eq!(state.key, "conf-1b1118ae25c9");
        assert!(state.is_legacy());
        assert_eq!(
            state.corpora.get("pdfjs"),
            Some(&CorpusState {
                failures: 3,
                documents: 2,
                missing: 0
            })
        );
        assert_eq!(legacy_signature(LEGACY), Some("render_error: boom @ x"));
    }

    #[test]
    fn round_trips_through_the_body() {
        let mut state = IssueState {
            key: key("mismatch: blank page"),
            corpora: BTreeMap::new(),
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
        assert_eq!(IssueState::parse("any", &body), Some(state.clone()));
        state.corpora.clear();
        let replaced = state.apply(&body).unwrap();
        assert_eq!(IssueState::parse("any", &replaced), Some(state));
        assert!(replaced.ends_with("\nBody text"));
        assert_eq!(replaced.matches(MARKER).count(), 1);
    }

    #[test]
    fn keys_ignore_the_corpus_and_keep_the_legacy_scheme() {
        assert!(key("x").starts_with(KEY_PREFIX));
        assert_eq!(key("x").len(), KEY_PREFIX.len() + 12);
        assert_ne!(legacy_key("pdfjs", "x"), legacy_key("pdfium", "x"));
        assert!(legacy_key("pdfjs", "x").starts_with("conf-"));
    }
}
