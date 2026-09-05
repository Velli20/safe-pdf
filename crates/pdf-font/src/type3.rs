//! PDF Type 3 font support backed by opaque character-procedure handles.
//!
//! Type 3 glyph programs are PDF content streams rather than standalone font
//! bytes. The PDF integration layer retains ownership of those streams and
//! gives this driver stable [`GlyphId`] handles that rendering backends
//! can resolve. PDF widths, bounds, encodings, and the font matrix remain in
//! the normalized PDF font specification instead of being duplicated here.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use crate::error::FontError;
use crate::font::{
    FontDriver, FontFace, FontFaceId, FontLoadRequest, FontMetadata, FontMetrics,
    FontProgramFormat, FontSource, GlyphGeometry, GlyphId, GlyphName,
};

/// A loader for PDF Type 3 faces represented by character-procedure handles.
pub struct Type3FontDriver;

impl Type3FontDriver {
    /// Creates a Type 3 driver.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for Type3FontDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl FontDriver for Type3FontDriver {
    /// Accepts only synthetic sources created from PDF Type 3 specifications.
    fn supports(&self, format: FontProgramFormat) -> bool {
        matches!(format, FontProgramFormat::Type3)
    }

    /// Loads a lightweight face that shares the request's character-procedure map.
    ///
    /// The face deliberately does not duplicate PDF widths, bounds, or the font matrix. Those
    /// values remain authoritative in the normalized font specification used by layout.
    fn load(&self, request: &FontLoadRequest) -> Result<Arc<dyn FontFace>, FontError> {
        let format =
            Option::<FontProgramFormat>::from(&request.source).unwrap_or(FontProgramFormat::Type3);
        if !self.supports(format) {
            return Err(FontError::DriverUnavailable { format });
        }

        let FontSource::Type3 { glyphs } = &request.source else {
            return Err(FontError::InvalidProgram {
                format,
                message: "the Type 3 driver requires character procedure handles".into(),
            });
        };

        Ok(Arc::new(Type3FontFace::new(
            request.face_id,
            request.metadata_hint.clone(),
            Arc::clone(glyphs),
        )))
    }
}

/// An immutable Type 3 face loaded by [`Type3FontDriver`].
pub struct Type3FontFace {
    /// Identity assigned by the driver that loaded this face.
    id: FontFaceId,
    /// Metadata supplied with the synthetic Type 3 source.
    metadata: FontMetadata,
    /// Original PDF procedure handles indexed by normalized glyph name.
    glyphs: Arc<BTreeMap<GlyphName, GlyphId>>,
    /// Procedure handles belonging to this face.
    glyph_ids: HashSet<GlyphId>,
}

impl Type3FontFace {
    /// Creates a face and indexes its opaque procedure handles by identifier.
    fn new(
        id: FontFaceId,
        metadata: FontMetadata,
        glyphs: Arc<BTreeMap<GlyphName, GlyphId>>,
    ) -> Self {
        // The name map supports PDF encoding lookup, while the identifier set
        // validates handles returned to rendering backends without repeatedly
        // scanning every character procedure.
        let glyph_ids = glyphs.values().copied().collect();
        Self {
            id,
            metadata,
            glyphs,
            glyph_ids,
        }
    }
}

impl FontFace for Type3FontFace {
    /// Returns the registry-assigned identity used to group glyph runs.
    fn id(&self) -> FontFaceId {
        self.id
    }

    /// Returns the metadata normalized from the PDF font dictionary.
    fn metadata(&self) -> &FontMetadata {
        &self.metadata
    }

    /// Returns no physical face metrics because PDF Type 3 metrics live in the font specification.
    fn metrics(&self) -> Option<FontMetrics> {
        None
    }

    /// Returns no Unicode lookup because Type 3 selection is driven by PDF encoding names.
    fn glyph_for_char(&self, _character: char) -> Option<GlyphId> {
        None
    }

    /// Resolves a normalized encoding name to its stable character-procedure handle.
    fn glyph_for_name(&self, name: &GlyphName) -> Option<GlyphId> {
        self.glyphs.get(name).copied()
    }

    /// Returns no physical advance because Type 3 widths belong to the PDF specification.
    fn horizontal_advance(&self, glyph: GlyphId) -> Result<Option<f32>, FontError> {
        self.glyph_ids
            .contains(&glyph)
            .then_some(None)
            .ok_or(FontError::MissingGlyph {
                face_id: self.id,
                glyph_id: glyph,
            })
    }

    /// Returns an opaque Type 3 handle after validating that it belongs to this face.
    fn glyph_geometry(
        &self,
        glyph: GlyphId,
        _pixels_per_em: f32,
    ) -> Result<Option<GlyphGeometry>, FontError> {
        Ok(self
            .glyph_ids
            .contains(&glyph)
            .then_some(GlyphGeometry::Type3(glyph)))
    }
}
