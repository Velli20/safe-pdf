//! Configured PDF font loading and text layout façade.
//!
//! Runtime fallback is mandatory because a PDF may omit a Standard 14 program or embed malformed
//! font data. Construction therefore takes the registry and fallback provider together:
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! use pdf_font::FontError;
//! use pdf_font::{
//!     FallbackCandidate, FallbackProvider, GlyphFallbackRequest, WholeFontFallbackRequest,
//! };
//! use pdf_font::FontRegistry;
//! use pdf_text_engine::system::FontSystem;
//!
//! struct EmptyFallback;
//!
//! impl FallbackProvider for EmptyFallback {
//!     fn whole_font_candidates(
//!         &self,
//!         _request: &WholeFontFallbackRequest<'_>,
//!     ) -> Result<Vec<FallbackCandidate>, FontError> {
//!         Ok(Vec::new())
//!     }
//!
//!     fn glyph_candidates(
//!         &self,
//!         _request: &GlyphFallbackRequest<'_>,
//!     ) -> Result<Vec<FallbackCandidate>, FontError> {
//!         Ok(Vec::new())
//!     }
//! }
//!
//! let registry = FontRegistry::new();
//! let _system = FontSystem::new(registry, Arc::new(EmptyFallback));
//! ```

use std::sync::Arc;

use crate::error::TextError;
use crate::text::{PdfTextRun, PositionedGlyph, TextLayout, TextVector};
use pdf_font::fallback::WholeFontFallbackRequest;
use pdf_font::pdf_font_handle::PdfFontHandle;
use pdf_font::{
    FallbackProvider, FontFace, FontMetadata, FontRegistry, FontSource, PdfFontSpec, Type3FontSpec,
};

/// Thread-safe façade for loading PDF fonts and producing positioned glyph layouts.
pub struct FontSystem {
    /// Ordered set of drivers used to turn owned font sources into faces.
    registry: FontRegistry,
    /// Application policy for whole-font and per-glyph substitution.
    fallback: Arc<dyn FallbackProvider>,
}

impl FontSystem {
    /// Creates a font system with its mandatory loading and fallback services.
    ///
    /// Both services are retained for the lifetime of the system. Sharing the fallback provider
    /// allows callers to reuse an immutable policy without another wrapper type.
    #[must_use]
    pub fn new(registry: FontRegistry, fallback: Arc<dyn FallbackProvider>) -> Self {
        Self { registry, fallback }
    }

    /// Returns the configured font driver registry.
    #[must_use]
    pub const fn registry(&self) -> &FontRegistry {
        &self.registry
    }

    /// Returns the configured fallback policy.
    #[must_use]
    pub fn fallback_provider(&self) -> &dyn FallbackProvider {
        self.fallback.as_ref()
    }

    /// Loads a normalized PDF font, applying whole-font substitution if necessary.
    ///
    /// Embedded and application-provided programs are tried first. Type 3 specifications are
    /// exposed to the registry as a shared character-procedure map, avoiding a clone of the map.
    /// A missing source or any primary load failure triggers candidates from the configured
    /// fallback policy. The returned handle records whether its primary face is a substitute so
    /// later glyph selection can prefer Unicode over incompatible PDF glyph identifiers.
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn load_pdf_font(&self, spec: PdfFontSpec) -> Result<PdfFontHandle, pdf_font::FontError> {
        let spec = Arc::new(spec);
        let source = match spec.as_ref() {
            PdfFontSpec::Type3(font) => Some(type3_source(font)),
            other => other.source().cloned(),
        };
        let primary = source.map(|source| self.load_with_metadata(source, spec.metadata().clone()));

        match primary.transpose() {
            Ok(Some(face)) => Ok(PdfFontHandle::new(spec, face)),
            Ok(None) | Err(_) => {
                let face = self.load_whole_font_fallback(spec.as_ref())?;
                Ok(PdfFontHandle::new_with_substitute(spec, face))
            }
        }
    }

    /// Loads an application font source for use as a PDF fallback face.
    pub fn load_font(&self, source: FontSource) -> Result<Arc<dyn FontFace>, pdf_font::FontError> {
        self.load_with_metadata(source, FontMetadata::default())
    }

    /// Decodes, resolves fallback, and positions a PDF text run.
    pub fn layout_pdf(&self, run: &PdfTextRun<'_>) -> Result<TextLayout, TextError> {
        crate::pdf_text_layout::layout_pdf(self, run)
    }

    /// Decodes and positions a PDF text run while emitting each glyph immediately.
    ///
    /// This avoids allocating a retained [`TextLayout`] when a caller only needs to paint or
    /// inspect glyphs once. The returned vector is the total text-space advance for the run.
    pub fn visit_pdf<E>(
        &self,
        run: &PdfTextRun<'_>,
        visitor: impl FnMut(&Arc<dyn FontFace>, PositionedGlyph) -> Result<(), E>,
    ) -> Result<TextVector, E>
    where
        E: From<TextError>,
    {
        crate::pdf_text_layout::visit_pdf(self, run, visitor)
    }

    /// Loads an owned source with metadata supplied by the PDF or fallback provider.
    ///
    /// Ownership is forwarded directly into the registry request, avoiding copies of font bytes
    /// and metadata at this boundary.
    pub(crate) fn load_with_metadata(
        &self,
        source: FontSource,
        metadata_hint: FontMetadata,
    ) -> Result<Arc<dyn FontFace>, pdf_font::FontError> {
        self.registry.load(source, metadata_hint)
    }

    /// Returns the first loadable whole-font fallback candidate in provider order.
    ///
    /// A candidate-specific load error does not stop the search because later candidates may use a
    /// different format or driver. If the provider supplies no loadable face, the error is
    /// normalized to [`FontError::FallbackExhausted`].
    fn load_whole_font_fallback(
        &self,
        spec: &PdfFontSpec,
    ) -> Result<Arc<dyn FontFace>, pdf_font::FontError> {
        let candidates = self
            .fallback
            .whole_font_candidates(&WholeFontFallbackRequest {
                pdf_font: spec,
                requested: spec.metadata(),
                excluded_faces: &[],
            })?;
        for candidate in candidates {
            if let Ok(face) = self.load_with_metadata(candidate.source, candidate.metadata) {
                return Ok(face);
            }
        }
        Err(pdf_font::FontError::FallbackExhausted)
    }
}

/// Creates a synthetic Type 3 source by sharing the specification's normalized name map.
///
/// Widths, bounds, and the font matrix remain in [`Type3FontSpec`]; the source contains only stable
/// glyph handles needed by the Type 3 driver and renderer.
fn type3_source(font: &Type3FontSpec) -> FontSource {
    FontSource::Type3 {
        glyphs: Arc::clone(&font.char_procedures),
    }
}
