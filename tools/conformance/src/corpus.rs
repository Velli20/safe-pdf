//! Corpus selection: locations, pinned revisions, and case discovery.

use crate::model::Case;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// A supported test corpus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum CorpusKind {
    /// PDFium's `pdfium_tests` repository.
    Pdfium,
    /// Mozilla pdf.js `test/pdfs` with `test/test_manifest.json`.
    Pdfjs,
}

impl CorpusKind {
    /// Returns the name used in paths and commands.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pdfium => "pdfium",
            Self::Pdfjs => "pdfjs",
        }
    }

    /// Returns the git URL and pinned commit of the corpus.
    pub fn source(self) -> (&'static str, &'static str) {
        match self {
            Self::Pdfium => (
                "https://pdfium.googlesource.com/pdfium_tests",
                "6a4136071608f159f45e274fed1b7d5f4272ca8f",
            ),
            Self::Pdfjs => (
                "https://github.com/mozilla/pdf.js",
                "df863ae9c48f6c44362622125288bd3de34e933f",
            ),
        }
    }

    /// Returns the sparse-checkout directories, or `None` for a full checkout.
    pub fn sparse_paths(self) -> Option<&'static [&'static str]> {
        match self {
            Self::Pdfium => None,
            Self::Pdfjs => Some(&["test/pdfs"]),
        }
    }

    /// Discovers the cases under `root`, sorted by id.
    pub fn cases(self, root: &Path) -> Result<Vec<Case>> {
        if !root.is_dir() {
            bail!(
                "corpus directory {} does not exist; run `cargo conformance fetch --corpus {}` or pass --root",
                root.display(),
                self.as_str()
            );
        }
        let mut cases = match self {
            Self::Pdfium => crate::corpus_pdfium::cases(root)?,
            Self::Pdfjs => crate::corpus_pdfjs::cases(root)?,
        };
        cases.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(cases)
    }
}

/// Returns the workspace root, fixed at compile time.
pub fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.ancestors().nth(2).unwrap_or(manifest).to_owned()
}

/// Returns the directory holding fetched corpora, PDFium, and run output.
pub fn state_dir() -> PathBuf {
    workspace_root().join("target").join("conformance")
}

/// Returns the default checkout location of a corpus.
pub fn default_root(kind: CorpusKind) -> PathBuf {
    state_dir().join("corpora").join(kind.as_str())
}

/// Returns the run output directory of a corpus.
pub fn output_dir(kind: CorpusKind) -> PathBuf {
    state_dir().join(kind.as_str())
}

/// Returns the checked-in baseline of a corpus.
pub fn baseline_path(kind: CorpusKind) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("baselines")
        .join(format!("{}.json", kind.as_str()))
}

/// Keeps cases whose id contains `filter` and, when `ids` is not empty, equals one of `ids`.
pub fn select(cases: Vec<Case>, filter: Option<&str>, ids: &[String]) -> Result<Vec<Case>> {
    let selected: Vec<Case> = cases
        .into_iter()
        .filter(|c| {
            filter.is_none_or(|f| c.id.contains(f)) && (ids.is_empty() || ids.contains(&c.id))
        })
        .collect();
    if selected.is_empty() {
        bail!("no cases match the selection");
    }
    Ok(selected)
}
