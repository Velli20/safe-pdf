//! Normalized PDF font resources, encodings, CMaps, and metrics.

use std::collections::BTreeMap;
use std::sync::Arc;

use pdf_graphics::rect::Rect;

use crate::base_encoding::BaseEncoding;
use crate::font::{FontMetadata, GlyphName};
pub use crate::pdf_font_spec::PdfFontSpec;
pub use crate::simple_font_spec::SimpleFontSpec;
pub use crate::standard14::Standard14Font;
pub use crate::type0_font_spec::{CidFontKind, CidFontSpec, Type0FontSpec};
pub use crate::type3_font_spec::Type3FontSpec;
pub use pdf_cmap::{PdfCode, ToUnicodeMap};

/// Number of PDF glyph-space units in one text-space em.
///
/// Except for Type 3 fonts, PDF glyph widths and text-positioning adjustments use
/// thousandths of text space. This value is also the conventional units-per-em
/// fallback for font programs whose physical metrics are missing or unusable.
pub const PDF_GLYPH_SPACE_UNITS_PER_EM: f32 = 1_000.0;

/// Simple-font encoding with optional code-to-name differences.
#[derive(Debug, Clone)]
pub struct SimpleEncoding {
    /// Base encoding applied before differences.
    pub base: BaseEncoding,
    /// Replacement glyph names indexed by one-byte character code.
    pub differences: BTreeMap<u8, GlyphName>,
}

/// Horizontal advance and optional vertical placement for one PDF character.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfGlyphMetric {
    /// Horizontal displacement in the font's PDF glyph space.
    pub advance_x: f32,
    /// Vertical displacement in the font's PDF glyph space.
    pub advance_y: f32,
    /// Horizontal coordinate of the vertical glyph origin.
    pub vertical_origin_x: Option<f32>,
    /// Vertical coordinate of the vertical glyph origin.
    pub vertical_origin_y: Option<f32>,
}

/// Sparse PDF metrics table with a required default.
///
/// Typed object decoding accepts a complete simple, Type 3, or descendant CID
/// font dictionary. Its subtype selects the width table and default-width rule.
#[derive(Debug, Clone)]
pub struct PdfMetrics {
    /// Metric used when no explicit entry exists.
    pub default: PdfGlyphMetric,
    /// Explicit metrics indexed by character code or CID.
    pub explicit: BTreeMap<u32, PdfGlyphMetric>,
}

/// Descriptor information shared by PDF font subtypes.
#[derive(Debug, Clone, Default)]
pub struct PdfFontDescriptor {
    /// Normalized font matching metadata.
    pub metadata: FontMetadata,
    /// Declared PDF font bounding box.
    pub bounds: Option<Rect>,
    /// Missing width in PDF glyph-space units.
    pub missing_width: Option<f32>,
    /// Italic angle in degrees.
    pub italic_angle: Option<f32>,
    /// Stem thickness hint.
    pub stem_v: Option<f32>,
}

/// Registry and ordering information from a CID font dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidSystemInfo {
    /// Registry string.
    pub registry: Arc<[u8]>,
    /// Ordering string.
    pub ordering: Arc<[u8]>,
    /// Supplement number.
    pub supplement: u32,
}
