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

/// The Safe-PDF stack whose render a run compares with the reference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Renderer {
    /// Page content through `PdfRenderer` onto a Skia raster; annotations are not drawn.
    #[default]
    Skia,
    /// The browser stack (`pdf-web`): page content on a Canvas 2D plus the annotation layer,
    /// rendered in headless Chrome.
    Web,
}

/// One kind of report: a corpus rendered by one renderer. Each has its own output
/// directory, baseline, published viewer and issue label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// Corpus the cases come from.
    pub corpus: CorpusKind,
    /// Renderer compared with the reference.
    pub renderer: Renderer,
}

impl Target {
    /// Every target, in the order reports are listed.
    pub const ALL: [Self; 4] = [
        Self::new(CorpusKind::Pdfium, Renderer::Skia),
        Self::new(CorpusKind::Pdfjs, Renderer::Skia),
        Self::new(CorpusKind::Pdfium, Renderer::Web),
        Self::new(CorpusKind::Pdfjs, Renderer::Web),
    ];

    /// Returns the target of `corpus` rendered by `renderer`.
    pub const fn new(corpus: CorpusKind, renderer: Renderer) -> Self {
        Self { corpus, renderer }
    }

    /// Returns the name used in paths, labels and commands: the corpus name, suffixed with
    /// `-web` for the web renderer.
    pub fn as_str(self) -> &'static str {
        match (self.corpus, self.renderer) {
            (CorpusKind::Pdfium, Renderer::Skia) => "pdfium",
            (CorpusKind::Pdfjs, Renderer::Skia) => "pdfjs",
            (CorpusKind::Pdfium, Renderer::Web) => "pdfium-web",
            (CorpusKind::Pdfjs, Renderer::Web) => "pdfjs-web",
        }
    }
}

impl clap::ValueEnum for Target {
    fn value_variants<'a>() -> &'a [Self] {
        &Self::ALL
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(clap::builder::PossibleValue::new(self.as_str()))
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

/// Returns the run output directory of a target.
pub fn output_dir(target: Target) -> PathBuf {
    state_dir().join(target.as_str())
}

/// Returns the checked-in baseline of a target.
pub fn baseline_path(target: Target) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("baselines")
        .join(format!("{}.json", target.as_str()))
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
