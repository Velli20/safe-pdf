//! Official PDFium golden images stored next to each PDF in pdfium_tests.
//!
//! PDFium's `pdfium_test` writes `<stem>_expected[_variant].pdf.<page>.png` at 72 dpi,
//! drawing annotations and form fields.

use std::path::{Path, PathBuf};

/// Variant suffixes in order of preference. PDFium's Skia variants come first because
/// Safe-PDF also rasterizes with Skia; GDI and AGG variants are never used.
fn variants() -> [String; 4] {
    let os = match std::env::consts::OS {
        "macos" => "mac",
        "windows" => "win",
        other => other,
    };
    [
        format!("_skia_{os}"),
        "_skia".to_owned(),
        format!("_{os}"),
        String::new(),
    ]
}

/// Returns the preferred golden of a page, if the corpus has one.
pub fn find(pdf: &Path, page: usize) -> Option<PathBuf> {
    let parent = pdf.parent()?;
    let stem = pdf.file_stem()?.to_string_lossy();
    variants()
        .iter()
        .map(|variant| parent.join(format!("{stem}_expected{variant}.pdf.{page}.png")))
        .find(|path| path.is_file())
}

/// Returns the number of consecutive pages, starting at 0, that have a golden.
pub fn page_count(pdf: &Path) -> usize {
    (0..).take_while(|page| find(pdf, *page).is_some()).count()
}
