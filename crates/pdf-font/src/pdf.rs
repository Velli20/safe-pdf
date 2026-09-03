//! Normalized PDF font resources, encodings, CMaps, and metrics.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use pdf_cmap::PdfCMap;
use pdf_content_stream::ContentStream;
use pdf_graphics::{rect::Rect, transform::Transform};

use crate::base_encoding::BaseEncoding;
use crate::font::{FontMetadata, FontSource, GlyphId, GlyphName};
pub use crate::pdf_font_spec::PdfFontSpec;
pub use crate::standard14::Standard14Font;
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

/// Data shared by Type 1, Multiple Master Type 1, and TrueType simple fonts.
#[derive(Clone)]
pub struct SimpleFontSpec {
    /// PDF base font name without a leading slash.
    pub base_font: Arc<[u8]>,
    /// Parsed descriptor information.
    pub descriptor: PdfFontDescriptor,
    /// Embedded, external, or Standard 14 program source when available.
    pub program: Option<FontSource>,
    /// Standard 14 identity when this resource denotes one of the built-in fonts.
    pub standard14: Option<Standard14Font>,
    /// One-byte character encoding.
    pub encoding: SimpleEncoding,
    /// Explicit PDF width data.
    pub metrics: PdfMetrics,
    /// Optional source-code-to-Unicode map.
    pub to_unicode: Option<Arc<dyn ToUnicodeMap>>,
}

/// Descendant subtype used by a Type 0 composite font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CidFontKind {
    /// CIDFontType0, normally backed by CID-keyed CFF outlines.
    Type0,
    /// CIDFontType2, backed by TrueType outlines.
    Type2,
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

/// Normalized descendant font used by a Type 0 composite font.
#[derive(Clone)]
pub struct CidFontSpec {
    /// Descendant subtype.
    pub kind: CidFontKind,
    /// Parsed descriptor information.
    pub descriptor: PdfFontDescriptor,
    /// Embedded or externally supplied descendant program.
    pub program: Option<FontSource>,
    /// CID collection identity.
    pub system_info: CidSystemInfo,
    /// Horizontal and vertical CID metrics.
    pub metrics: PdfMetrics,
    /// Optional CID-to-glyph identifier mapping for CIDFontType2.
    pub cid_to_gid: Option<Arc<[u16]>>,
    /// Best-effort CID-to-Unicode mapping for collection-backed font substitution.
    pub cid_to_unicode: Option<Arc<HashMap<u16, char>>>,
}

/// Normalized Type 0 composite font.
#[derive(Clone)]
pub struct Type0FontSpec {
    /// PDF base font name without a leading slash.
    pub base_font: Arc<[u8]>,
    /// Source-code-to-CID encoding CMap.
    pub encoding: Arc<dyn PdfCMap>,
    /// The single descendant CID font.
    pub descendant: CidFontSpec,
    /// Optional source-code-to-Unicode map.
    pub to_unicode: Option<Arc<dyn ToUnicodeMap>>,
}

/// Normalized Type 3 font whose glyph programs remain owned by the PDF layer.
#[derive(Clone)]
pub struct Type3FontSpec {
    /// PDF base font name without a leading slash.
    pub base_font: Arc<[u8]>,
    /// Font matching metadata inferred from the PDF resource.
    pub metadata: FontMetadata,
    /// Matrix mapping Type 3 glyph space to text space.
    pub font_matrix: Transform,
    /// Declared Type 3 font bounds.
    pub bounds: Rect,
    /// One-byte character encoding.
    pub encoding: SimpleEncoding,
    /// PDF width data.
    pub metrics: PdfMetrics,
    /// Opaque character procedure handles indexed by glyph name.
    pub char_procedures: Arc<BTreeMap<GlyphName, GlyphId>>,
    /// Parsed PDF content streams indexed by their opaque glyph handles.
    pub type3_procedures: Arc<HashMap<GlyphId, ContentStream>>,
    /// Optional source-code-to-Unicode map.
    pub to_unicode: Option<Arc<dyn ToUnicodeMap>>,
}
