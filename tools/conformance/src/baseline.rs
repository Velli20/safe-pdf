//! Checked-in expected outcomes, used to separate regressions from known failures.

use crate::model::{CaseResult, Status};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

/// Expected outcome of one page.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BaselinePage {
    /// Expected status.
    pub status: Status,
    /// Expected mismatch fraction, rounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mismatch: Option<f64>,
    /// Signature when the page fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Expected outcome of one case.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BaselineCase {
    /// Expected document status.
    pub status: Status,
    /// Expected page outcomes keyed by zero-based page index.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pages: BTreeMap<usize, BaselinePage>,
}

/// All expected outcomes of a corpus.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Baseline {
    /// PDFium library identity the baseline was recorded with.
    pub pdfium: String,
    /// Corpus revision the baseline was recorded with.
    pub corpus_revision: Option<String>,
    /// Expected outcomes by case id.
    pub cases: BTreeMap<String, BaselineCase>,
}

/// Change of an outcome relative to the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delta {
    /// Worse than the baseline.
    Regressed,
    /// Better than the baseline.
    Improved,
    /// Same as the baseline within tolerance.
    Unchanged,
    /// Not in the baseline.
    New,
}

impl Baseline {
    /// Loads a baseline, or an empty one when the file does not exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    /// Writes the baseline with stable ordering.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        fs::write(path, text)?;
        Ok(())
    }

    /// Replaces the expected outcome of a case with the observed one.
    pub fn record(&mut self, result: &CaseResult) {
        let pages = result
            .pages
            .iter()
            .map(|page| {
                (
                    page.page,
                    BaselinePage {
                        status: page.status,
                        mismatch: page
                            .output
                            .as_ref()
                            .and_then(|output| output.metrics.as_ref())
                            .map(|metrics| (metrics.mismatch * 100_000.0).round() / 100_000.0),
                        signature: page.signature.clone(),
                    },
                )
            })
            .collect();
        self.cases.insert(
            result.case.id.clone(),
            BaselineCase {
                status: result.status,
                pages,
            },
        );
    }

    /// Compares one page with its expected outcome.
    pub fn page_delta(
        &self,
        case: &str,
        page: usize,
        status: Status,
        mismatch: Option<f64>,
    ) -> Delta {
        let Some(expected) = self.cases.get(case).and_then(|case| case.pages.get(&page)) else {
            return Delta::New;
        };
        compare(expected.status, expected.mismatch, status, mismatch)
    }

    /// Compares a document-level outcome with its expected outcome.
    pub fn case_delta(&self, case: &str, status: Status) -> Delta {
        match self.cases.get(case) {
            Some(expected) => compare(expected.status, None, status, None),
            None => Delta::New,
        }
    }
}

fn compare(
    old: Status,
    old_mismatch: Option<f64>,
    new: Status,
    new_mismatch: Option<f64>,
) -> Delta {
    match new.severity().cmp(&old.severity()) {
        std::cmp::Ordering::Greater => Delta::Regressed,
        std::cmp::Ordering::Less => Delta::Improved,
        std::cmp::Ordering::Equal => match (old_mismatch, new_mismatch) {
            (Some(old), Some(new)) if new > old * 1.25 + 0.002 => Delta::Regressed,
            (Some(old), Some(new)) if new < old * 0.8 - 0.002 => Delta::Improved,
            _ => Delta::Unchanged,
        },
    }
}
