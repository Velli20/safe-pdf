//! Runtime whole-font substitution and missing-glyph fallback contracts.

use pdf_object_reader::{DictionaryContext, ObjectAccess};

use crate::error::FontError;
use crate::font::{FontFaceId, FontMetadata, FontSource};
use crate::pdf_font_spec::PdfFontSpec;

/// Request for a complete substitute before PDF text decoding begins.
pub struct WholeFontFallbackRequest<'a> {
    /// Normalized PDF resource whose primary program is unusable.
    pub pdf_font: &'a PdfFontSpec,
    /// Metadata used for family and style matching.
    pub requested: &'a FontMetadata,
    /// Faces already attempted during this resolution operation.
    pub excluded_faces: &'a [FontFaceId],
}

/// Request for candidate faces covering one missing Unicode scalar.
pub struct GlyphFallbackRequest<'a> {
    /// Missing Unicode scalar.
    pub character: char,
    /// Metadata of the primary or requested face.
    pub requested: &'a FontMetadata,
    /// Source PDF font when fallback occurs in a PDF run.
    pub pdf_font: Option<&'a PdfFontSpec>,
    /// Faces already attempted for this glyph.
    pub excluded_faces: &'a [FontFaceId],
}

/// Ordered source proposed by a fallback provider.
#[derive(Debug, Clone)]
pub struct FallbackCandidate {
    /// Font program source to attempt.
    pub source: FontSource,
    /// Matching metadata used by fallback selection and diagnostics.
    pub metadata: FontMetadata,
}

/// Application-supplied policy for runtime font substitution.
pub trait FallbackProvider: Send + Sync {
    /// Returns ordered candidates that can replace an unusable primary font.
    fn whole_font_candidates(
        &self,
        request: &WholeFontFallbackRequest<'_>,
    ) -> Result<Vec<FallbackCandidate>, FontError>;

    /// Returns ordered candidates expected to cover one missing scalar.
    fn glyph_candidates(
        &self,
        request: &GlyphFallbackRequest<'_>,
    ) -> Result<Vec<FallbackCandidate>, FontError>;
}

/// Selects the historical whole-font fallback from BaseFont, without descriptor flags.
pub(crate) fn fallback_standard14_font(
    context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>,
) -> crate::standard14::Standard14Font {
    crate::standard14::from_context(context, crate::flags::FontFlags::empty())
}
