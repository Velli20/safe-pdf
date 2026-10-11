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

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::error::TextError;
use crate::text::{PdfTextRun, PositionedGlyph, TextLayout, TextVector};
use pdf_font::fallback::WholeFontFallbackRequest;
use pdf_font::pdf_font_handle::PdfFontHandle;
use pdf_font::{
    FallbackCandidate, FallbackProvider, FontFace, FontMetadata, FontProgramFormat, FontRegistry,
    FontSource, PdfFontSpec, Type3FontSpec,
};

/// Identity of an in-memory fallback program: its buffer address and length, format, and face.
///
/// Keying by address avoids hashing large bundled programs. Each cache entry retains its source,
/// so the buffer stays alive and its address cannot be reused by another program.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FallbackSourceKey {
    address: usize,
    length: usize,
    format: FontProgramFormat,
    face_index: u32,
}

impl FallbackSourceKey {
    /// Returns the key of an in-memory source, or `None` for sources that are not cached.
    fn from_source(source: &FontSource) -> Option<Self> {
        let FontSource::Memory {
            data,
            format,
            face_index,
        } = source
        else {
            return None;
        };
        Some(Self {
            address: data.as_ptr().addr(),
            length: data.len(),
            format: *format,
            face_index: *face_index,
        })
    }
}

/// Glyph-fallback faces loaded so far, with the sources that keep their buffers alive.
type FallbackFaces = HashMap<FallbackSourceKey, (FontSource, Arc<dyn FontFace>)>;

/// Thread-safe façade for loading PDF fonts and producing positioned glyph layouts.
pub struct FontSystem {
    /// Ordered set of drivers used to turn owned font sources into faces.
    registry: FontRegistry,
    /// Application policy for whole-font and per-glyph substitution.
    fallback: Arc<dyn FallbackProvider>,
    /// Glyph-fallback faces, loaded once so runs and pages share faces and their glyph caches.
    fallback_faces: RwLock<FallbackFaces>,
}

impl FontSystem {
    /// Creates a font system with its mandatory loading and fallback services.
    ///
    /// Both services are retained for the lifetime of the system. Sharing the fallback provider
    /// allows callers to reuse an immutable policy without another wrapper type.
    #[must_use]
    pub fn new(registry: FontRegistry, fallback: Arc<dyn FallbackProvider>) -> Self {
        Self {
            registry,
            fallback,
            fallback_faces: RwLock::new(HashMap::new()),
        }
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

    /// Loads a glyph-fallback candidate, reusing the face loaded earlier for the same program.
    ///
    /// In-memory candidates are cached for the lifetime of the system; other sources load anew.
    /// Fallback face metadata only describes the substitute, so one face serves every request.
    pub(crate) fn load_fallback_face(
        &self,
        candidate: FallbackCandidate,
    ) -> Result<Arc<dyn FontFace>, pdf_font::FontError> {
        let Some(key) = FallbackSourceKey::from_source(&candidate.source) else {
            return self.load_with_metadata(candidate.source, candidate.metadata);
        };
        // Entries are inserted only after their face has loaded, so a poisoned map is still valid.
        let cached = match self.fallback_faces.read() {
            Ok(faces) => faces.get(&key).map(|(_, face)| Arc::clone(face)),
            Err(poisoned) => poisoned
                .into_inner()
                .get(&key)
                .map(|(_, face)| Arc::clone(face)),
        };
        if let Some(face) = cached {
            return Ok(face);
        }
        let face = self.load_with_metadata(candidate.source.clone(), candidate.metadata)?;
        let entry = (candidate.source, Arc::clone(&face));
        match self.fallback_faces.write() {
            Ok(mut faces) => faces.insert(key, entry),
            Err(poisoned) => poisoned.into_inner().insert(key, entry),
        };
        Ok(face)
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
