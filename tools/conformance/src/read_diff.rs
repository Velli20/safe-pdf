//! Read outcomes of a run (`reads.json`) and their comparison with another run's, used to
//! report what a pull request changes in how documents are read.

use crate::{
    corpus::{self, CorpusKind},
    fetch,
    model::{CaseResult, Status},
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Write as _, fs, path::Path};

/// Read outcomes written next to every run.
pub const READS_FILE: &str = "reads.json";
/// Markdown section describing the changes.
const DIFF_MARKDOWN: &str = "read-diff.md";
/// Counts of the changes, for scripts.
const DIFF_JSON: &str = "read-diff.json";
/// Rows listed per table; the rest are counted.
const ROW_LIMIT: usize = 50;

/// How one document was read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadEntry {
    /// `pass` when Safe-PDF read the document; otherwise `read_error`, `crash`, `timeout` or
    /// `unavailable`.
    pub status: Status,
    /// Failure cluster key of a failed read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Safe-PDF page count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_pages: Option<usize>,
    /// Reference page count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_pages: Option<usize>,
}

impl ReadEntry {
    /// Returns the read outcome of a case, ignoring how its pages rendered.
    fn from_result(result: &CaseResult) -> Self {
        let status = match &result.read {
            None => result.status,
            Some(read) if read.safe_error.is_some() => Status::ReadError,
            Some(_) => Status::Pass,
        };
        Self {
            status,
            signature: status
                .is_failure()
                .then(|| result.signature.clone())
                .flatten(),
            safe_pages: result.read.as_ref().and_then(|read| read.safe_pages),
            reference_pages: result.read.as_ref().and_then(|read| read.reference_pages),
        }
    }

    /// Returns true when both renderers counted pages and the counts differ.
    fn page_count_differs(&self) -> bool {
        matches!((self.safe_pages, self.reference_pages), (Some(safe), Some(reference)) if safe != reference)
    }

    /// Returns how bad the outcome is, higher is worse, or `None` when the file was missing.
    fn rank(&self) -> Option<u8> {
        Some(match self.status {
            Status::Unavailable => return None,
            Status::ReadError => 2,
            Status::Timeout => 3,
            Status::Crash => 4,
            _ if self.page_count_differs() => 1,
            _ => 0,
        })
    }

    /// Describes the outcome in one table cell.
    fn describe(&self) -> String {
        let text = match (self.status, &self.signature) {
            (Status::Pass, _) => match (self.safe_pages, self.reference_pages) {
                (Some(safe), Some(reference)) if safe != reference => {
                    format!("read, {} (PDFium: {reference})", pages(safe))
                }
                (Some(safe), _) => format!("read, {}", pages(safe)),
                (None, _) => "read".to_owned(),
            },
            (_, Some(signature)) => signature.clone(),
            (status, None) => status.as_str().to_owned(),
        };
        text.replace('|', "\\|").replace(['\n', '\r'], " ")
    }
}

/// Read outcomes of one run.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Reads {
    /// Corpus name.
    pub corpus: String,
    /// Commit of the Safe-PDF checkout that ran.
    pub commit: Option<String>,
    /// Outcomes by case id.
    pub cases: BTreeMap<String, ReadEntry>,
}

impl Reads {
    /// Collects the read outcomes of a run.
    pub fn from_results(kind: CorpusKind, results: &[CaseResult]) -> Self {
        Self {
            corpus: kind.as_str().to_owned(),
            commit: fetch::revision(&corpus::workspace_root()),
            cases: results
                .iter()
                .map(|result| (result.case.id.clone(), ReadEntry::from_result(result)))
                .collect(),
        }
    }

    /// Writes the outcomes as JSON.
    pub fn save(&self, path: &Path) -> Result<()> {
        fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

/// One document whose read outcome changed.
#[derive(Debug)]
struct Change<'a> {
    id: &'a str,
    base: &'a ReadEntry,
    head: &'a ReadEntry,
}

/// Documents read worse and better than the base run.
#[derive(Debug, Default)]
struct Diff<'a> {
    compared: usize,
    regressed: Vec<Change<'a>>,
    improved: Vec<Change<'a>>,
}

/// Counts written to `read-diff.json`.
#[derive(Serialize)]
struct DiffCounts<'a> {
    corpus: &'a str,
    base_commit: Option<&'a str>,
    head_commit: Option<&'a str>,
    compared: usize,
    regressed: usize,
    improved: usize,
}

/// Compares documents present and available in both runs.
fn diff<'a>(base: &'a Reads, head: &'a Reads) -> Diff<'a> {
    let mut diff = Diff::default();
    for (id, head_entry) in &head.cases {
        let Some(base_entry) = base.cases.get(id) else {
            continue;
        };
        let (Some(old), Some(new)) = (base_entry.rank(), head_entry.rank()) else {
            continue;
        };
        diff.compared = diff.compared.saturating_add(1);
        let change = Change {
            id,
            base: base_entry,
            head: head_entry,
        };
        match new.cmp(&old) {
            std::cmp::Ordering::Greater => diff.regressed.push(change),
            std::cmp::Ordering::Less => diff.improved.push(change),
            std::cmp::Ordering::Equal => {}
        }
    }
    diff
}

fn pages(count: usize) -> String {
    if count == 1 {
        "1 page".to_owned()
    } else {
        format!("{count} pages")
    }
}

fn short(commit: Option<&str>) -> &str {
    commit.map_or("unknown", |commit| commit.get(..7).unwrap_or(commit))
}

fn corpus_title(corpus: &str) -> &str {
    match corpus {
        "pdfium" => "PDFium corpus",
        "pdfjs" => "pdf.js corpus",
        other => other,
    }
}

/// Renders the changes as a Markdown section.
fn markdown(base: &Reads, head: &Reads, diff: &Diff<'_>) -> Result<String> {
    let mut text = String::new();
    writeln!(
        text,
        "### {}\n\n{} documents compared with `{}`: {} read worse, {} read better.\n",
        corpus_title(&head.corpus),
        diff.compared,
        short(base.commit.as_deref()),
        diff.regressed.len(),
        diff.improved.len()
    )?;
    for (title, changes) in [
        ("Regressions", &diff.regressed),
        ("Improvements", &diff.improved),
    ] {
        if changes.is_empty() {
            continue;
        }
        writeln!(
            text,
            "**{title}**\n\n| Document | Base | This pull request |\n|---|---|---|"
        )?;
        for change in changes.iter().take(ROW_LIMIT) {
            writeln!(
                text,
                "| `{}` | {} | {} |",
                change.id.replace('`', "'"),
                change.base.describe(),
                change.head.describe()
            )?;
        }
        if changes.len() > ROW_LIMIT {
            writeln!(
                text,
                "\n…and {} more.",
                changes.len().saturating_sub(ROW_LIMIT)
            )?;
        }
        writeln!(text)?;
    }
    Ok(text)
}

/// Compares the last run of `kind` with the reads at `base_path` and writes the report.
pub fn run(kind: CorpusKind, base_path: &Path) -> Result<()> {
    let out = corpus::output_dir(kind);
    let head = Reads::load(&out.join(READS_FILE))?;
    let base = Reads::load(base_path)?;
    let diff = diff(&base, &head);
    fs::write(out.join(DIFF_MARKDOWN), markdown(&base, &head, &diff)?)?;
    let counts = DiffCounts {
        corpus: &head.corpus,
        base_commit: base.commit.as_deref(),
        head_commit: head.commit.as_deref(),
        compared: diff.compared,
        regressed: diff.regressed.len(),
        improved: diff.improved.len(),
    };
    fs::write(out.join(DIFF_JSON), serde_json::to_vec_pretty(&counts)?)?;
    println!(
        "{}: {} documents compared with {}, {} read worse, {} read better. Report: {}",
        corpus_title(&head.corpus),
        diff.compared,
        short(base.commit.as_deref()),
        diff.regressed.len(),
        diff.improved.len(),
        out.join(DIFF_MARKDOWN).display()
    );
    for change in &diff.regressed {
        println!(
            "  regressed {}: {} -> {}",
            change.id,
            change.base.describe(),
            change.head.describe()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(status: Status, safe: Option<usize>, reference: Option<usize>) -> ReadEntry {
        ReadEntry {
            status,
            signature: status
                .is_failure()
                .then(|| format!("{}: boom", status.as_str())),
            safe_pages: safe,
            reference_pages: reference,
        }
    }

    fn reads(cases: &[(&str, ReadEntry)]) -> Reads {
        Reads {
            corpus: "pdfjs".to_owned(),
            commit: Some("0123456789abcdef".to_owned()),
            cases: cases
                .iter()
                .map(|(id, entry)| ((*id).to_owned(), entry.clone()))
                .collect(),
        }
    }

    #[test]
    fn sorts_changes_into_regressions_and_improvements() {
        let base = reads(&[
            ("broke", entry(Status::Pass, Some(2), Some(2))),
            ("fixed", entry(Status::ReadError, None, Some(1))),
            ("count", entry(Status::Pass, Some(3), Some(3))),
            ("same", entry(Status::ReadError, None, Some(1))),
            ("missing", entry(Status::Unavailable, None, None)),
            ("gone", entry(Status::Pass, Some(1), Some(1))),
        ]);
        let head = reads(&[
            ("broke", entry(Status::Crash, None, None)),
            ("fixed", entry(Status::Pass, Some(1), Some(1))),
            ("count", entry(Status::Pass, Some(2), Some(3))),
            ("same", entry(Status::ReadError, None, Some(1))),
            ("missing", entry(Status::Pass, Some(1), Some(1))),
            ("new", entry(Status::Crash, None, None)),
        ]);
        let diff = diff(&base, &head);
        assert_eq!(diff.compared, 4);
        let regressed: Vec<&str> = diff.regressed.iter().map(|change| change.id).collect();
        let improved: Vec<&str> = diff.improved.iter().map(|change| change.id).collect();
        assert_eq!(regressed, ["broke", "count"]);
        assert_eq!(improved, ["fixed"]);
    }

    #[test]
    fn markdown_lists_changes_with_escaped_cells() {
        let mut broken = entry(Status::ReadError, None, Some(1));
        broken.signature = Some("read_error: bad | token".to_owned());
        let base = reads(&[("a", entry(Status::Pass, Some(1), Some(1)))]);
        let head = reads(&[("a", broken)]);
        let text = markdown(&base, &head, &diff(&base, &head)).unwrap();
        assert!(text.contains("### pdf.js corpus"));
        assert!(text.contains("compared with `0123456`: 1 read worse, 0 read better"));
        assert!(text.contains("| `a` | read, 1 page | read_error: bad \\| token |"));
        assert!(!text.contains("Improvements"));
    }
}
