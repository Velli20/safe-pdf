//! Complete normalized PDF font resource specifications.

use pdf_object_reader::pdf_name::PdfName;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};
use std::collections::BTreeMap;
use std::sync::Arc;

use pdf_content_stream::ContentStream;

use crate::base_encoding::BaseEncoding;
use crate::error::FontError;
use crate::flags::FontFlags;
use crate::font::GlyphId;
use crate::font::LineMetrics;
use crate::font::{FontMetadata, FontSource};
use crate::glyph_widths_map::GlyphWidthsMap;
use crate::pdf::{
    FALLBACK_GLYPH_WIDTH, PdfFontDescriptor, PdfMetrics, SimpleEncoding, SimpleFontSpec,
    Type0FontSpec, Type3FontSpec,
};
use crate::simple_font_spec::SimpleFontKind;
use crate::standard14::{self, Standard14Font};
use pdf_cmap::WritingMode;

/// Complete normalized representation of a PDF font resource.
#[derive(Clone)]
pub enum PdfFontSpec {
    /// A Type 0 composite font.
    Type0(Type0FontSpec),
    /// A simple Type 1, Multiple Master Type 1, or TrueType font.
    Simple(SimpleFontSpec),
    /// A Type 3 font whose glyphs are PDF content streams.
    Type3(Type3FontSpec),
}

/// The font dictionary layout a `/Subtype` name selects.
enum FontSubtype {
    /// `/Type0`.
    Type0,
    /// `/Type1`, `/MMType1`, or `/TrueType`.
    Simple,
    /// `/Type3`.
    Type3,
}

impl FromPdfObject for FontSubtype {
    /// Decodes a font dictionary's `/Subtype` name.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let name = context.decode::<PdfName>()?;
        match &*name {
            b"Type0" => Ok(Self::Type0),
            b"Type1" | b"MMType1" | b"TrueType" => Ok(Self::Simple),
            b"Type3" => Ok(Self::Type3),
            other => Err(FontError::UnsupportedFontSubtype {
                subtype: String::from_utf8_lossy(other).into_owned(),
            }
            .into()),
        }
    }
}

impl FromPdfObject for PdfFontSpec {
    /// Reads a font resource, substituting a Standard 14 font on parsing failure.
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let mut entries = context.entries()?;
        let subtype = entries.lenient::<FontSubtype>(b"Subtype");
        let base_font = entries.lenient::<PdfName>(b"BaseFont");
        // Keep recovery at the resource boundary: a missing or unsupported subtype, or a
        // concrete font that fails to parse, falls back to a Standard 14 font.
        let spec = match subtype {
            Some(FontSubtype::Type0) => context.decode().map(Self::Type0).ok(),
            Some(FontSubtype::Simple) => context.decode().map(Self::Simple).ok(),
            Some(FontSubtype::Type3) => context.decode().map(Self::Type3).ok(),
            None => None,
        };
        // The whole-font fallback follows BaseFont alone, without descriptor flags.
        Ok(spec.unwrap_or_else(|| {
            Self::from(standard14::select(base_font.as_deref(), FontFlags::empty()))
        }))
    }
}

impl From<Standard14Font> for PdfFontSpec {
    fn from(standard14: Standard14Font) -> Self {
        Self::Simple(SimpleFontSpec {
            kind: SimpleFontKind::Type1,
            base_font: Arc::from(standard14.to_string().into_bytes()),
            descriptor: PdfFontDescriptor::default(),
            program: None,
            standard14: Some(standard14),
            encoding: SimpleEncoding {
                base: BaseEncoding::Standard,
                differences: BTreeMap::new(),
            },
            metrics: PdfMetrics {
                default: FALLBACK_GLYPH_WIDTH,
                explicit: GlyphWidthsMap::default(),
            },
            to_unicode: None,
        })
    }
}

impl PdfFontSpec {
    /// Borrows the normalized metadata associated with this PDF font.
    ///
    /// Borrowing avoids cloning names and classification data during face loading and fallback
    /// queries. Type 0 fonts expose their descendant descriptor metadata; Type 3 fonts retain their
    /// own normalized matching hint.
    #[must_use]
    pub fn metadata(&self) -> &FontMetadata {
        match self {
            Self::Type0(font) => &font.descendant.descriptor.metadata,
            Self::Simple(font) => &font.descriptor.metadata,
            Self::Type3(font) => &font.metadata,
        }
    }

    /// The line extent the font descriptor declares; `None` for a Type 3 font, which has
    /// no descriptor metrics, and for a descriptor that declares none.
    #[must_use]
    pub fn declared_line_metrics(&self) -> Option<LineMetrics> {
        match self {
            Self::Type0(font) => font.descendant.descriptor.line_metrics(),
            Self::Simple(font) => font.descriptor.line_metrics(),
            Self::Type3(_) => None,
        }
    }

    /// Returns the embedded or externally supplied font program, when available.
    #[must_use]
    pub fn source(&self) -> Option<&FontSource> {
        match self {
            Self::Type0(font) => font.descendant.program.as_ref(),
            Self::Simple(font) => font.program.as_ref(),
            Self::Type3(_) => None,
        }
    }

    /// Returns the writing direction used to lay out this PDF font.
    #[must_use]
    pub fn writing_mode(&self) -> WritingMode {
        match self {
            Self::Type0(font) => font.encoding.writing_mode(),
            _ => WritingMode::Horizontal,
        }
    }

    /// Returns whether this font contains Type 3 character procedures.
    #[must_use]
    pub fn is_type3(&self) -> bool {
        matches!(self, Self::Type3(_))
    }

    /// Resolves a Type 3 glyph handle to its PDF content stream.
    #[must_use]
    pub fn type3_procedure(&self, glyph: GlyphId) -> Option<&ContentStream> {
        let Self::Type3(font) = self else {
            return None;
        };
        font.char_procedures.get(glyph)
    }

    /// Returns the bundled Standard 14 identity, when present.
    #[must_use]
    pub fn as_standard14(&self) -> Option<Standard14Font> {
        match self {
            Self::Simple(font) => font.standard14,
            _ => None,
        }
    }

    /// Returns whether this font uses one of the predefined CJK CID collections.
    #[must_use]
    pub fn is_cjk_spec(&self) -> bool {
        let Self::Type0(font) = self else {
            return false;
        };
        matches!(
            font.descendant.system_info.ordering.as_ref(),
            b"Japan1" | b"GB1" | b"CNS1" | b"Korea1"
        )
    }

    /// Returns the Standard 14 identity whose bundled face substitutes for this font.
    pub(crate) fn substitute_standard14(&self) -> Standard14Font {
        match self {
            Self::Simple(font) => font.standard14.unwrap_or_default(),
            Self::Type0(font) => {
                standard14::from_base_font_name(&font.base_font).unwrap_or_default()
            }
            Self::Type3(_) => Standard14Font::Helvetica,
        }
    }
}
