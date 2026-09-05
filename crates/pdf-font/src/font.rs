//! Font program, face, registry, and glyph geometry abstractions.

use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::Bytes;
use pdf_graphics::pdf_path::PdfPath;

use crate::error::FontError;
pub use crate::font_registry::FontRegistry;

/// Stable identity assigned to one loaded font face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontFaceId(pub u64);

/// Font-specific identifier for one glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlyphId(pub u32);

/// Opaque key understood by an application-provided external font source.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalFontKey(pub Arc<str>);

/// Supported physical font program representations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontProgramFormat {
    /// A classic PostScript Type 1 program.
    Type1,
    /// A name-keyed Compact Font Format program, including PDF `/Type1C` data.
    Cff,
    /// A CID-keyed Compact Font Format program, including PDF `/CIDFontType0C` data.
    CidCff,
    /// A standalone TrueType program.
    TrueType,
    /// An OpenType container with TrueType outlines.
    OpenTypeTrueType,
    /// An OpenType container with CFF or CFF2 outlines.
    OpenTypeCff,
    /// A PDF Type 3 program whose glyphs are content streams rather than font bytes.
    Type3,
}

/// Origin and storage for a font program requested by the loader.
#[derive(Debug, Clone)]
pub enum FontSource {
    /// Font bytes embedded in a PDF or supplied directly by the application.
    Memory {
        /// Shared immutable program bytes.
        data: Bytes,
        /// Declared or detected program format.
        format: FontProgramFormat,
        /// Zero-based face index for collection containers.
        face_index: u32,
    },
    /// A font that must be obtained from an application-owned database.
    External {
        /// Provider-specific lookup key.
        key: ExternalFontKey,
        /// Expected program format when it is known.
        format_hint: Option<FontProgramFormat>,
        /// Zero-based face index for collection containers.
        face_index: u32,
    },
    /// A PDF Type 3 face represented by normalized glyph procedure handles.
    Type3 {
        /// Procedure handles indexed by glyph name.
        glyphs: Arc<BTreeMap<GlyphName, GlyphId>>,
    },
}

impl From<&FontSource> for Option<FontProgramFormat> {
    /// Extracts an explicitly declared format from a font source.
    ///
    /// External sources may intentionally omit a format hint, in which case
    /// the caller must supply the format it is attempting to load. Keeping
    /// that fallback choice in the driver preserves useful diagnostics for
    /// each driver's source-specific error path.
    fn from(source: &FontSource) -> Self {
        match source {
            FontSource::Memory { format, .. }
            | FontSource::External {
                format_hint: Some(format),
                ..
            } => Some(*format),
            FontSource::External {
                format_hint: None, ..
            } => None,
            FontSource::Type3 { .. } => Some(FontProgramFormat::Type3),
        }
    }
}

/// Slant requested from or reported by a font face.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum FontSlant {
    /// Upright roman glyphs.
    #[default]
    Normal,
    /// Designed italic glyphs.
    Italic,
    /// Mechanically or descriptively oblique glyphs.
    Oblique {
        /// Slant angle in degrees when known.
        angle: Option<f32>,
    },
}

/// Width class requested from or reported by a font face.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FontStretch {
    /// A face narrower than the normal width class.
    Condensed,
    /// The normal width class.
    #[default]
    Normal,
    /// A face wider than the normal width class.
    Expanded,
}

/// CSS-compatible numeric font weight in the inclusive conceptual range 1–1000.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontWeight(pub u16);

impl Default for FontWeight {
    fn default() -> Self {
        Self(400)
    }
}

/// Human-readable and stylistic information associated with a loaded face.
#[derive(Debug, Clone, Default)]
pub struct FontMetadata {
    /// PostScript name when one is available.
    pub postscript_name: Option<Arc<str>>,
    /// Typographic family name when one is available.
    pub family: Option<Arc<str>>,
    /// Typographic subfamily name when one is available.
    pub subfamily: Option<Arc<str>>,
    /// Numeric weight used during fallback matching.
    pub weight: FontWeight,
    /// Width class used during fallback matching.
    pub stretch: FontStretch,
    /// Slant used during fallback matching.
    pub slant: FontSlant,
    /// Whether the face is treated as symbolic rather than Unicode-oriented.
    pub symbolic: bool,
}

/// A normalized glyph name without a leading PDF slash.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlyphName(pub Arc<[u8]>);

/// Global metrics expressed in font design units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    /// Number of design units per em square.
    pub units_per_em: u16,
    /// Typographic ascender.
    pub ascender: f32,
    /// Typographic descender.
    pub descender: f32,
}

/// Fallback ascender measured as a fraction of one em above the baseline.
///
/// This is the conventional 80/20 division of an em-square line box: 80% is
/// reserved above the baseline and 20% below it. It is not a metric prescribed
/// by the PDF or OpenType specifications and is used only when a font provides
/// neither usable line metrics nor usable global bounds.
pub const FALLBACK_ASCENDER_EM_RATIO: f32 = 0.8;

/// Fallback descender measured as a fraction of one em below the baseline.
///
/// Together with [`FALLBACK_ASCENDER_EM_RATIO`], this produces a one-em-high
/// selection cell using the conventional 80/20 baseline split. The value is
/// negative because font descenders are expressed below the baseline.
pub const FALLBACK_DESCENDER_EM_RATIO: f32 = -0.2;

/// Renderable geometry supplied by a loaded font face.
#[derive(Clone)]
pub enum GlyphGeometry {
    /// A scalable vector outline in font design units.
    Outline(PdfPath),
    /// A PDF Type 3 character procedure delegated to the integration backend.
    Type3(GlyphId),
}

/// Parameters passed to a format driver while loading a font face.
#[derive(Debug, Clone)]
pub struct FontLoadRequest {
    /// Program source to load.
    pub source: FontSource,
    /// Metadata inferred from the enclosing PDF or application request.
    pub metadata_hint: FontMetadata,
    /// Registry-allocated identity for the face being loaded.
    pub face_id: FontFaceId,
}

/// A parsed, immutable font face usable by PDF layout and rendering workers.
pub trait FontFace: Send + Sync {
    /// Returns the stable identity assigned to this face.
    fn id(&self) -> FontFaceId;

    /// Returns descriptive and matching metadata.
    fn metadata(&self) -> &FontMetadata;

    /// Returns global design-space metrics when the font provides them.
    fn metrics(&self) -> Option<FontMetrics>;

    /// Resolves a Unicode scalar to a glyph identifier when the face covers it.
    fn glyph_for_char(&self, character: char) -> Option<GlyphId>;

    /// Resolves a PostScript glyph name when supported by the face.
    fn glyph_for_name(&self, name: &GlyphName) -> Option<GlyphId>;

    /// Returns the glyph's horizontal advance in font design units when available.
    ///
    /// PDF layout uses this only when an unrelated fallback face supplies the glyph. Native PDF
    /// faces continue to use widths from the PDF font dictionary.
    fn horizontal_advance(&self, glyph: GlyphId) -> Result<Option<f32>, FontError>;

    /// Returns renderable geometry for one glyph at the requested pixel size.
    fn glyph_geometry(
        &self,
        glyph: GlyphId,
        pixels_per_em: f32,
    ) -> Result<Option<GlyphGeometry>, FontError>;
}

/// Format-specific loader that turns font sources into immutable faces.
pub trait FontDriver: Send + Sync {
    /// Reports whether this driver accepts the supplied program format.
    fn supports(&self, format: FontProgramFormat) -> bool;

    /// Loads one face from a normalized request.
    fn load(&self, request: &FontLoadRequest) -> Result<Arc<dyn FontFace>, FontError>;
}
