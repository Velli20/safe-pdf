//! Run-wide outputs: `index.json`, `results.json`, `TRIAGE.md`, and the HTML viewer.

use crate::{
    baseline::{Baseline, Delta},
    fetch,
    model::{CaseResult, Status},
    run::RunOptions,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    path::Path,
};

const VIEWER: &str = include_str!("viewer.html");

/// One page row of the index.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexPage {
    /// Zero-based page index.
    pub page: usize,
    /// Page verdict.
    pub status: Status,
    /// Mismatch fraction.
    pub mismatch: Option<f64>,
    /// Failure cluster key.
    pub signature: Option<String>,
    /// Change against the baseline.
    pub delta: Delta,
    /// Baseline verdict, when recorded.
    pub baseline: Option<Status>,
    /// Evidence files relative to the case directory.
    pub files: Vec<String>,
    /// Command rerunning this page.
    pub reproduce: String,
}

/// One case row of the index.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexCase {
    /// Case id.
    pub id: String,
    /// Case directory relative to the run output.
    pub dir: String,
    /// Document verdict.
    pub status: Status,
    /// Document-level failure cluster key.
    pub signature: Option<String>,
    /// Document-level change against the baseline.
    pub delta: Delta,
    /// Baseline verdict, when recorded.
    pub baseline: Option<Status>,
    /// Corpus note.
    pub note: Option<String>,
    /// Likely crates from the feature inventory.
    pub likely_crates: Vec<String>,
    /// Command rerunning this case.
    pub reproduce: String,
    /// True when a `summary.md` bundle was written.
    pub bundle: bool,
    /// Checked pages.
    pub pages: Vec<IndexPage>,
}

/// Failures sharing a signature.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cluster {
    /// Shared signature.
    pub signature: String,
    /// Failing pages (or documents without pages) with this signature.
    pub count: usize,
    /// Case ids, in id order.
    pub cases: Vec<String>,
    /// Likely crates across the cluster, most frequent first.
    pub likely_crates: Vec<(String, usize)>,
    /// Signatures of the same failures under the clustering used before root-cause
    /// signatures, used to find issues filed with them.
    #[serde(default)]
    pub legacy_signatures: Vec<String>,
}

/// The run index.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Index {
    /// Corpus name.
    pub corpus: String,
    /// Corpus checkout.
    pub corpus_root: String,
    /// Corpus commit.
    pub corpus_revision: Option<String>,
    /// Reference description: golden images, or the PDFium library's name and SHA-256 prefix.
    pub pdfium: String,
    /// Pixels per PDF point.
    pub scale: f32,
    /// Mismatch fraction treated as a pass.
    pub tolerance: f64,
    /// True when only part of the corpus was checked (`--filter`, `--case` or `--page`).
    #[serde(default)]
    pub filtered: bool,
    /// Documents per verdict.
    pub totals: BTreeMap<String, usize>,
    /// Pages per verdict.
    pub page_totals: BTreeMap<String, usize>,
    /// Failure clusters, largest first.
    pub clusters: Vec<Cluster>,
    /// Cases in id order.
    pub cases: Vec<IndexCase>,
}

/// Builds the index, comparing each outcome with the baseline.
pub fn build(options: &RunOptions, results: &[CaseResult], baseline: &Baseline) -> Index {
    let mut totals = BTreeMap::<String, usize>::new();
    let mut page_totals = BTreeMap::<String, usize>::new();
    let mut clusters = BTreeMap::<String, ClusterAccumulator>::new();
    let mut cases = Vec::with_capacity(results.len());
    for result in results {
        bump(&mut totals, result.status.as_str());
        let likely_crates = result
            .read
            .as_ref()
            .map(|read| read.inventory.likely_crates.clone())
            .unwrap_or_default();
        let mut add_to_cluster = |signature: &str, legacy: Option<&String>| {
            let entry = clusters.entry(signature.to_owned()).or_default();
            entry.count = entry.count.saturating_add(1);
            entry.cases.insert(result.case.id.clone());
            entry.legacy.extend(legacy.cloned());
            for name in &likely_crates {
                bump(&mut entry.crates, name);
            }
        };
        let pages: Vec<IndexPage> = result
            .pages
            .iter()
            .map(|page| {
                bump(&mut page_totals, page.status.as_str());
                if page.status.is_failure()
                    && let Some(signature) = &page.signature
                {
                    add_to_cluster(signature, page.legacy_signature.as_ref());
                }
                let mismatch = page
                    .output
                    .as_ref()
                    .and_then(|output| output.metrics.as_ref())
                    .map(|metrics| metrics.mismatch);
                IndexPage {
                    page: page.page,
                    status: page.status,
                    mismatch,
                    signature: page.signature.clone(),
                    delta: baseline.page_delta(&result.case.id, page.page, page.status, mismatch),
                    baseline: baseline
                        .cases
                        .get(&result.case.id)
                        .and_then(|case| case.pages.get(&page.page))
                        .map(|page| page.status),
                    files: page
                        .output
                        .as_ref()
                        .map(|output| output.files.clone())
                        .unwrap_or_default(),
                    reproduce: options.reproduce(&result.case.id, Some(page.page)),
                }
            })
            .collect();
        if result.status.is_failure()
            && result.pages.iter().all(|page| !page.status.is_failure())
            && let Some(signature) = &result.signature
        {
            add_to_cluster(signature, result.legacy_signature.as_ref());
        }
        cases.push(IndexCase {
            id: result.case.id.clone(),
            dir: result.dir.clone(),
            status: result.status,
            signature: result.signature.clone(),
            delta: baseline.case_delta(&result.case.id, result.status),
            baseline: baseline.cases.get(&result.case.id).map(|case| case.status),
            note: result.case.note.clone(),
            likely_crates,
            reproduce: options.reproduce(&result.case.id, None),
            bundle: needs_bundle(result),
            pages,
        });
    }
    let mut clusters: Vec<Cluster> = clusters
        .into_iter()
        .map(|(signature, accumulator)| {
            let mut likely_crates: Vec<(String, usize)> = accumulator.crates.into_iter().collect();
            likely_crates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            Cluster {
                signature,
                count: accumulator.count,
                cases: accumulator.cases.into_iter().collect(),
                likely_crates,
                legacy_signatures: accumulator.legacy.into_iter().collect(),
            }
        })
        .collect();
    clusters.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.signature.cmp(&b.signature))
    });
    Index {
        corpus: options.kind.as_str().to_owned(),
        corpus_root: options.root.display().to_string(),
        corpus_revision: fetch::revision(&options.root),
        pdfium: options.pdfium.as_ref().map_or_else(
            || "official pdfium_tests golden images".to_owned(),
            |library| {
                fetch::pdfium_identity(library)
                    .map_or_else(|error| error.to_string(), |id| format!("PDFium {id}"))
            },
        ),
        scale: options.scale,
        tolerance: options.tolerance,
        filtered: options.filtered(),
        totals,
        page_totals,
        clusters,
        cases,
    }
}

/// Failures collected for one signature while building the index.
#[derive(Default)]
struct ClusterAccumulator {
    count: usize,
    cases: BTreeSet<String>,
    crates: BTreeMap<String, usize>,
    legacy: BTreeSet<String>,
}

/// Returns true when a case gets a `summary.md` bundle.
pub fn needs_bundle(result: &CaseResult) -> bool {
    result.status.is_failure() || result.pages.iter().any(|page| page.status.is_failure())
}

fn bump(map: &mut BTreeMap<String, usize>, key: &str) {
    let entry = map.entry(key.to_owned()).or_default();
    *entry = entry.saturating_add(1);
}

impl Index {
    /// Returns the cluster holding `signature`.
    pub fn cluster(&self, signature: &str) -> Option<&Cluster> {
        self.clusters
            .iter()
            .find(|cluster| cluster.signature == signature)
    }

    /// Describes outcomes worse than the baseline, and new crashes or timeouts.
    pub fn regressions(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let is_new_crash = |delta: Delta, status: Status| {
            delta == Delta::New && matches!(status, Status::Crash | Status::Timeout)
        };
        for case in &self.cases {
            let describe = |page: Option<usize>,
                            old: Option<Status>,
                            new: Status,
                            signature: Option<&String>| {
                format!(
                    "{}{}: {} -> {}{}",
                    case.id,
                    page.map(|page| format!(" page {page}")).unwrap_or_default(),
                    old.map_or("new", Status::as_str),
                    new.as_str(),
                    signature.map(|s| format!(" ({s})")).unwrap_or_default()
                )
            };
            if case.pages.is_empty() {
                if case.delta == Delta::Regressed || is_new_crash(case.delta, case.status) {
                    lines.push(describe(
                        None,
                        case.baseline,
                        case.status,
                        case.signature.as_ref(),
                    ));
                }
                continue;
            }
            for page in &case.pages {
                if page.delta == Delta::Regressed || is_new_crash(page.delta, page.status) {
                    lines.push(describe(
                        Some(page.page),
                        page.baseline,
                        page.status,
                        page.signature.as_ref(),
                    ));
                }
            }
        }
        lines
    }

    /// Returns a one-line summary of verdicts.
    pub fn totals_line(&self) -> String {
        let join = |map: &BTreeMap<String, usize>| {
            map.iter()
                .map(|(status, count)| format!("{count} {status}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "{} documents: {}. Pages: {}.",
            self.cases.len(),
            join(&self.totals),
            join(&self.page_totals)
        )
    }
}

/// Writes `index.json`, `index.js`, `index.html`, and `TRIAGE.md`.
pub fn write(out: &Path, index: &Index) -> Result<()> {
    let json = serde_json::to_string(index)?;
    fs::write(out.join("index.json"), serde_json::to_string_pretty(index)?)?;
    fs::write(
        out.join("index.js"),
        format!("window.CONFORMANCE = {json};\n"),
    )?;
    fs::write(out.join("index.html"), VIEWER)?;
    fs::write(out.join("TRIAGE.md"), triage(index)?)?;
    Ok(())
}

fn triage(index: &Index) -> Result<String> {
    let mut text = String::new();
    writeln!(text, "# {} conformance triage\n", index.corpus)?;
    writeln!(
        text,
        "Reference: {} ({} px/pt unless golden images fix the size). Pass at ≤{:.2}% mismatched pixels. Corpus: `{}` at {}.\n",
        index.pdfium,
        index.scale,
        index.tolerance * 100.0,
        index.corpus_root,
        index
            .corpus_revision
            .as_deref()
            .unwrap_or("unknown revision")
    )?;
    writeln!(text, "{}\n", index.totals_line())?;
    let regressions = index.regressions();
    if !regressions.is_empty() {
        writeln!(text, "## Regressions against the baseline\n")?;
        for line in &regressions {
            writeln!(text, "- {line}")?;
        }
        writeln!(text)?;
    }
    let substituted = index
        .page_totals
        .get(Status::FontSubstitution.as_str())
        .copied()
        .unwrap_or(0);
    if substituted > 0 {
        writeln!(
            text,
            "{substituted} pages differ only in text drawn with non-embedded fonts, which renderers substitute differently. \
             They are reported as `font_substitution`, count as expected, and are not filed as issues.\n"
        )?;
    }
    writeln!(
        text,
        "## Failure clusters\n\nEach cluster groups failures with the same signature, so one fix may resolve all of them. \
         Start with the largest cluster. Each case links to its `summary.md`.\n"
    )?;
    for cluster in &index.clusters {
        writeln!(
            text,
            "### {} failures in {} documents: `{}`\n",
            cluster.count,
            cluster.cases.len(),
            cluster.signature
        )?;
        if !cluster.likely_crates.is_empty() {
            let crates: Vec<String> = cluster
                .likely_crates
                .iter()
                .take(6)
                .map(|(name, count)| format!("{name} ({count})"))
                .collect();
            writeln!(text, "Features point at: {}\n", crates.join(", "))?;
        }
        for id in cluster.cases.iter().take(25) {
            if let Some(case) = index.cases.iter().find(|case| &case.id == id) {
                let worst = case
                    .pages
                    .iter()
                    .filter_map(|page| page.mismatch)
                    .fold(None, |max: Option<f64>, value| {
                        Some(max.map_or(value, |m| m.max(value)))
                    });
                writeln!(
                    text,
                    "- [{}](cases/{}/summary.md){}",
                    case.id,
                    case.dir,
                    worst
                        .map(|value| format!(" (mismatch {:.2}%)", value * 100.0))
                        .unwrap_or_default()
                )?;
            }
        }
        if cluster.cases.len() > 25 {
            writeln!(
                text,
                "- … {} more in index.json",
                cluster.cases.len().saturating_sub(25)
            )?;
        }
        writeln!(text)?;
    }
    Ok(text)
}
