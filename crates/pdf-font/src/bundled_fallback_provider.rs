//! Whole-font and missing-glyph fallback from the faces bundled with this crate.

use bytes::Bytes;

use crate::error::FontError;
use crate::fallback::{
    FallbackCandidate, FallbackProvider, GlyphFallbackRequest, WholeFontFallbackRequest,
};
use crate::font::{FontProgramFormat, FontSource};
use crate::pdf_font_spec::PdfFontSpec;
use crate::standard14;

const NOTO_SANS_CJK_JP_REGULAR: &[u8] = include_bytes!("../assets/NotoSansCJKjp-Regular.otf");
const NOTO_SANS_HEBREW_REGULAR: &[u8] = include_bytes!("../assets/NotoSansHebrew-Regular.otf");

/// Bundled whole-font and missing-glyph fallback policy.
pub struct BundledFallbackProvider;

impl FallbackProvider for BundledFallbackProvider {
    fn whole_font_candidates(
        &self,
        request: &WholeFontFallbackRequest<'_>,
    ) -> Result<Vec<FallbackCandidate>, FontError> {
        Ok(vec![request.pdf_font.fallback_candidate()])
    }

    fn glyph_candidates(
        &self,
        request: &GlyphFallbackRequest<'_>,
    ) -> Result<Vec<FallbackCandidate>, FontError> {
        let Some(pdf_font) = request.pdf_font else {
            return Ok(Vec::new());
        };
        let mut candidates = vec![pdf_font.fallback_candidate()];
        // The CJK face also covers Greek, Cyrillic, and symbols, such as card suits, that the
        // Western Latin substitutes lack.
        if !pdf_font.is_cjk_spec() {
            candidates.push(FallbackCandidate::cjk());
        }
        // Neither the Latin substitutes nor the CJK face cover Hebrew.
        if matches!(request.character, '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}') {
            candidates.push(FallbackCandidate::memory(
                Bytes::from_static(NOTO_SANS_HEBREW_REGULAR),
                FontProgramFormat::OpenTypeCff,
            ));
        }
        Ok(candidates)
    }
}

impl PdfFontSpec {
    /// Returns the bundled face that substitutes for this whole font: the CJK face for a
    /// CJK collection, and otherwise its Standard 14 substitute.
    fn fallback_candidate(&self) -> FallbackCandidate {
        if self.is_cjk_spec() {
            return FallbackCandidate::cjk();
        }
        FallbackCandidate::memory(
            Bytes::from_static(standard14::fallback_font_bytes(
                self.substitute_standard14(),
            )),
            FontProgramFormat::Cff,
        )
    }
}

impl FallbackCandidate {
    /// Returns the bundled Noto Sans CJK face as a fallback candidate.
    fn cjk() -> Self {
        Self::memory(
            Bytes::from_static(NOTO_SANS_CJK_JP_REGULAR),
            FontProgramFormat::OpenTypeCff,
        )
    }

    /// Wraps an in-memory font program as a fallback candidate.
    fn memory(data: Bytes, format: FontProgramFormat) -> Self {
        Self {
            source: FontSource::Memory {
                data,
                format,
                face_index: 0,
            },
        }
    }
}
