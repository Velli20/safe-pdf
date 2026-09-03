//! Complete normalized PDF font resource specifications.

use std::sync::Arc;

use bytes::Bytes;
use pdf_content_stream::ContentStream;

use crate::error::FontError;
use crate::fallback::{
    FallbackCandidate, FallbackProvider, GlyphFallbackRequest, WholeFontFallbackRequest,
};
use crate::font::GlyphId;
use crate::font::{FontMetadata, FontProgramFormat, FontSource};
use crate::pdf::{
    PdfFontDescriptor, PdfGlyphMetric, PdfMetrics, SimpleEncoding, SimpleFontSpec, Type0FontSpec,
    Type3FontSpec,
};
use crate::standard14::Standard14Font;
use pdf_cmap::WritingMode;

const NOTO_SANS_CJK_JP_REGULAR: &[u8] =
    include_bytes!("../../pdf-font/assets/NotoSansCJKjp-Regular.otf");

/// Complete normalized representation of a PDF font resource.
#[derive(Clone)]
pub enum PdfFontSpec {
    /// A Type 0 composite font.
    Type0(Type0FontSpec),
    /// A simple Type 1 font.
    Type1(SimpleFontSpec),
    /// A simple Multiple Master Type 1 font.
    MultipleMasterType1(SimpleFontSpec),
    /// A simple TrueType font, including OpenType-wrapped TrueType programs.
    TrueType(SimpleFontSpec),
    /// A Type 3 font whose glyphs are PDF content streams.
    Type3(Type3FontSpec),
}

impl From<Standard14Font> for PdfFontSpec {
    fn from(standard14: Standard14Font) -> Self {
        Self::Type1(SimpleFontSpec {
            base_font: Arc::from(standard14.to_string().into_bytes()),
            descriptor: PdfFontDescriptor::default(),
            program: None,
            standard14: Some(standard14),
            encoding: SimpleEncoding {
                base: crate::base_encoding::BaseEncoding::Standard,
                differences: std::collections::BTreeMap::new(),
            },
            metrics: PdfMetrics {
                default: PdfGlyphMetric {
                    advance_x: 500.0,
                    advance_y: 0.0,
                    vertical_origin_x: None,
                    vertical_origin_y: None,
                },
                explicit: std::collections::BTreeMap::new(),
            },
            to_unicode: None,
        })
    }
}

impl PdfFontSpec {
    /// Returns the PDF metric for a character code or CID.
    ///
    /// Sparse explicit entries take precedence; the normalized table's required default is
    /// returned for every missing code, so callers do not need another optional layer.
    #[must_use]
    pub fn metric(&self, code: u32) -> PdfGlyphMetric {
        let metrics = match self {
            Self::Type0(font) => &font.descendant.metrics,
            Self::Type1(font) | Self::MultipleMasterType1(font) | Self::TrueType(font) => {
                &font.metrics
            }
            Self::Type3(font) => &font.metrics,
        };
        metrics
            .explicit
            .get(&code)
            .copied()
            .unwrap_or(metrics.default)
    }

    /// Borrows the normalized metadata associated with this PDF font.
    ///
    /// Borrowing avoids cloning names and classification data during face loading and fallback
    /// queries. Type 0 fonts expose their descendant descriptor metadata; Type 3 fonts retain their
    /// own normalized matching hint.
    #[must_use]
    pub fn metadata(&self) -> &FontMetadata {
        match self {
            Self::Type0(font) => &font.descendant.descriptor.metadata,
            Self::Type1(font) | Self::MultipleMasterType1(font) | Self::TrueType(font) => {
                &font.descriptor.metadata
            }
            Self::Type3(font) => &font.metadata,
        }
    }

    /// Returns the embedded or externally supplied font program, when available.
    #[must_use]
    pub fn source(&self) -> Option<&FontSource> {
        match self {
            Self::Type0(font) => font.descendant.program.as_ref(),
            Self::Type1(font) | Self::MultipleMasterType1(font) | Self::TrueType(font) => {
                font.program.as_ref()
            }
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
        font.type3_procedures.get(&glyph)
    }

    /// Returns the bundled Standard 14 identity, when present.
    #[must_use]
    pub fn as_standard14(&self) -> Option<Standard14Font> {
        let font = match self {
            Self::Type1(font) | Self::MultipleMasterType1(font) | Self::TrueType(font) => {
                font.standard14?
            }
            _ => return None,
        };
        Some(font)
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
}

/// Bundled whole-font and missing-glyph fallback policy.
pub struct BundledFallbackProvider;

impl FallbackProvider for BundledFallbackProvider {
    fn whole_font_candidates(
        &self,
        request: &WholeFontFallbackRequest<'_>,
    ) -> Result<Vec<FallbackCandidate>, FontError> {
        Ok(vec![fallback_candidate(
            request.pdf_font,
            request.requested,
        )])
    }

    fn glyph_candidates(
        &self,
        request: &GlyphFallbackRequest<'_>,
    ) -> Result<Vec<FallbackCandidate>, FontError> {
        let Some(pdf_font) = request.pdf_font else {
            return Ok(Vec::new());
        };
        Ok(vec![fallback_candidate(pdf_font, request.requested)])
    }
}

fn fallback_candidate(spec: &PdfFontSpec, metadata: &FontMetadata) -> FallbackCandidate {
    let standard14 = match spec {
        PdfFontSpec::Type1(font)
        | PdfFontSpec::MultipleMasterType1(font)
        | PdfFontSpec::TrueType(font) => font.standard14.unwrap_or_default(),
        _ => Standard14Font::Helvetica,
    };
    let (data, format) = if spec.is_cjk_spec() {
        (
            Bytes::from_static(NOTO_SANS_CJK_JP_REGULAR),
            FontProgramFormat::OpenTypeCff,
        )
    } else {
        (
            Bytes::from_static(crate::standard14::fallback_font_bytes(standard14)),
            FontProgramFormat::TrueType,
        )
    };
    FallbackCandidate {
        source: FontSource::Memory {
            data,
            format,
            face_index: 0,
        },
        metadata: metadata.clone(),
    }
}
