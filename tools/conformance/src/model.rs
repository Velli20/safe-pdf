//! Serializable evidence exchanged between workers, reports, and baselines.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One document selected from a corpus, with the pages the corpus asks to check.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    /// Stable corpus identifier used for filtering, baselines, and reproduction.
    pub id: String,
    /// Absolute path of the PDF file.
    pub path: PathBuf,
    /// Document password from the corpus manifest.
    pub password: Option<String>,
    /// First page to check, zero-based and inclusive.
    pub first_page: Option<usize>,
    /// Last page to check, zero-based and inclusive.
    pub last_page: Option<usize>,
    /// MD5 digest recorded by the corpus manifest.
    pub expected_md5: Option<String>,
    /// Corpus-specific context shown to readers, such as the pdf.js test type.
    pub note: Option<String>,
}

/// An error rendered with its source chain and the backtrace captured nearest to its origin.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ErrorDetail {
    /// The top-level error message.
    pub message: String,
    /// Messages of every `source()` below the top-level error.
    pub chain: Vec<String>,
    /// The full symbolized backtrace, when one was captured.
    pub backtrace: Option<String>,
}

/// A recoverable read diagnostic reported by Safe-PDF.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Diagnostic category name.
    pub kind: String,
    /// Zero-based page associated with the diagnostic.
    pub page: Option<u32>,
    /// Indirect object as `number generation`.
    pub object: Option<String>,
    /// Byte offset in the file.
    pub byte_offset: Option<usize>,
    /// Human-readable description of the underlying error.
    pub message: String,
    /// Stack captured when the diagnostic was constructed.
    pub backtrace: Option<String>,
}

/// PDF features found in a document, used to point readers at the crates most likely involved.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Inventory {
    /// Feature name mapped to the number of objects using it, sorted by name.
    pub features: Vec<(String, usize)>,
    /// Names of fonts without embedded font programs.
    pub non_embedded_fonts: Vec<String>,
    /// Crates associated with the features found.
    pub likely_crates: Vec<String>,
}

/// Output of the document read worker.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReadOutput {
    /// Safe-PDF page count, when the document loaded.
    pub safe_pages: Option<usize>,
    /// Fatal Safe-PDF read error.
    pub safe_error: Option<ErrorDetail>,
    /// Recoverable Safe-PDF read diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Features found in the loaded object graph.
    pub inventory: Inventory,
    /// PDFium page count, when PDFium could open the document.
    pub reference_pages: Option<usize>,
    /// PDFium open failure.
    pub reference_error: Option<String>,
}

/// Pixel comparison scores for one page.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metrics {
    /// Fraction of pixels that differ after tolerating anti-aliasing.
    pub mismatch: f64,
    /// Mean absolute RGB channel difference, 0 to 255.
    pub mean_abs_rgb: f64,
    /// Fraction of reference pixels that are ink (clearly darker than paper).
    pub reference_ink: f64,
    /// Fraction of Safe-PDF pixels that are ink.
    pub safe_ink: f64,
    /// Fraction of pixels not compared because annotations cover them in the reference.
    #[serde(default)]
    pub ignored: f64,
}

/// How a differing region deviates from the reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionClass {
    /// The reference has content that Safe-PDF did not draw.
    MissingInk,
    /// Safe-PDF drew content the reference does not have.
    ExtraInk,
    /// Both drew here but colors differ.
    ColorShift,
    /// Both drew here but shapes or positions differ (glyph substitution, geometry).
    Reshaped,
    /// The same content is drawn at a small offset.
    Offset,
}

impl RegionClass {
    /// Returns the snake-case name used in signatures and reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingInk => "missing_ink",
            Self::ExtraInk => "extra_ink",
            Self::ColorShift => "color_shift",
            Self::Reshaped => "reshaped",
            Self::Offset => "offset",
        }
    }
}

/// A Safe-PDF backend drawing call overlapping a differing region.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawRef {
    /// Position of the call in the page's drawing sequence.
    pub seq: usize,
    /// Call kind: fill, stroke, image, inline_image.
    pub kind: String,
    /// Paint details such as color, shader, image size, or blend mode.
    pub detail: String,
    /// Device-space bounds `[x0, y0, x1, y1]` after clipping.
    pub bounds: [f32; 4],
}

/// A connected area of differing pixels with what each renderer drew there.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Region {
    /// Pixel bounds `[x0, y0, x1, y1]`, exclusive of `x1`/`y1`.
    pub pixels: [u32; 4],
    /// PDF user-space bounds `[left, bottom, right, top]` from PDFium's page mapping.
    pub page_space: Option<[f32; 4]>,
    /// Number of differing pixels.
    pub mismatched: u64,
    /// Dominant deviation.
    pub class: RegionClass,
    /// Offset `(dx, dy)` that best aligns Safe-PDF with the reference, when classified as offset.
    pub shift: Option<[i32; 2]>,
    /// Text PDFium extracts inside the region.
    pub reference_text: String,
    /// Text Safe-PDF laid out inside the region.
    pub safe_text: String,
    /// Safe-PDF drawing calls overlapping the region, largest overlap first.
    pub draws: Vec<DrawRef>,
}

/// Output of one page worker.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PageOutput {
    /// Zero-based page index.
    pub page: usize,
    /// Raster size used by both renderers.
    pub size: [u32; 2],
    /// PDFium rendering failure.
    pub reference_error: Option<String>,
    /// Safe-PDF read or rendering failure.
    pub safe_error: Option<ErrorDetail>,
    /// Comparison scores, when both renders succeeded.
    pub metrics: Option<Metrics>,
    /// Differing regions, most mismatched first.
    pub regions: Vec<Region>,
    /// Number of Safe-PDF backend drawing calls on the page.
    pub draw_count: usize,
    /// Image and text files written to the case directory, relative to it.
    pub files: Vec<String>,
}

/// Process-level evidence for one worker run.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProcessEvidence {
    /// Exit status as printed by the operating system.
    pub exit_status: String,
    /// True when the worker was killed for exceeding the timeout.
    pub timed_out: bool,
    /// Wall time in milliseconds.
    pub elapsed_ms: u128,
    /// Last worker stage reached (`reference`, `safe-read`, `safe-render`, ...).
    pub stage: Option<String>,
    /// Captured standard error, truncated from the front.
    pub stderr: String,
    /// Worker stdout when it could not be parsed.
    pub stdout: Option<String>,
}

/// Outcome category for a page or a whole document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Safe-PDF matches the reference within tolerance.
    Pass,
    /// Only text differs, in a document with non-embedded fonts that renderers substitute
    /// differently. Expected, so not a failure.
    FontSubstitution,
    /// PDFium could not render the page; nothing to compare against.
    NoReference,
    /// The PDF file is not available locally.
    Unavailable,
    /// Rendering differs from the reference.
    Mismatch,
    /// Safe-PDF returned an error while rendering the page.
    RenderError,
    /// Safe-PDF returned an error while reading the document.
    ReadError,
    /// A worker exited abnormally (panic, abort, signal).
    Crash,
    /// A worker exceeded the time limit.
    Timeout,
}

impl Status {
    /// Returns the snake-case name used in reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::FontSubstitution => "font_substitution",
            Self::NoReference => "no_reference",
            Self::Unavailable => "unavailable",
            Self::Mismatch => "mismatch",
            Self::RenderError => "render_error",
            Self::ReadError => "read_error",
            Self::Crash => "crash",
            Self::Timeout => "timeout",
        }
    }

    /// Returns true when the status needs attention.
    pub fn is_failure(self) -> bool {
        !matches!(
            self,
            Self::Pass | Self::FontSubstitution | Self::NoReference | Self::Unavailable
        )
    }

    /// Returns a severity rank; higher is worse.
    pub fn severity(self) -> u8 {
        match self {
            Self::Pass | Self::FontSubstitution | Self::NoReference | Self::Unavailable => 0,
            Self::Mismatch => 1,
            Self::RenderError => 2,
            Self::ReadError => 3,
            Self::Timeout => 4,
            Self::Crash => 5,
        }
    }
}

/// Evidence and verdict for one page.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageResult {
    /// Page verdict.
    pub status: Status,
    /// Worker output, absent when the worker crashed or timed out.
    pub output: Option<PageOutput>,
    /// Page index, also present when `output` is absent.
    pub page: usize,
    /// Worker process evidence.
    pub process: ProcessEvidence,
    /// Failure cluster key.
    pub signature: Option<String>,
    /// Signature under the clustering used before root-cause signatures, used to find
    /// issues filed with it.
    #[serde(default)]
    pub legacy_signature: Option<String>,
}

/// Everything known about one case after a run.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseResult {
    /// The case that was run.
    pub case: Case,
    /// Document-level verdict (worst page verdict or read failure).
    pub status: Status,
    /// SHA-256 of the PDF bytes.
    pub sha256: Option<String>,
    /// True when the file's MD5 does not match the manifest.
    pub md5_mismatch: bool,
    /// Read worker output.
    pub read: Option<ReadOutput>,
    /// Read worker process evidence.
    pub read_process: Option<ProcessEvidence>,
    /// Document-level failure cluster key.
    pub signature: Option<String>,
    /// Document-level signature under the clustering used before root-cause signatures.
    #[serde(default)]
    pub legacy_signature: Option<String>,
    /// Per-page results.
    pub pages: Vec<PageResult>,
    /// Case directory relative to the run output directory.
    pub dir: String,
}
