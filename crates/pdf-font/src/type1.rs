//! Classic PostScript Type 1 font support backed by `skrifa`'s raw parser.
//!
//! A Type 1 program stores glyphs as PostScript charstrings rather than as
//! OpenType tables. The low-level parser is re-exported by `skrifa` as
//! `skrifa::raw`; this module uses that parser and the shared outline-pen
//! interface to keep the rest of the text engine independent of the font
//! format.
//!
//! Programs are parsed into owned font data when loaded. Glyph outlines and
//! metrics are subsequently evaluated in unscaled font design coordinates,
//! matching the contract used by the TrueType driver. Text sizing and device
//! transforms are deliberately left to the layout and rendering layers.

use std::sync::Arc;

use read_fonts::{model::pen::NullPen, ps::type1::Type1Font as ClassicType1Program};

use crate::error::FontError;
use crate::font::{
    FontDriver, FontFace, FontFaceId, FontLoadRequest, FontMetadata, FontMetrics,
    FontProgramFormat, FontSource, GlyphGeometry, GlyphId, GlyphName,
};
use crate::pdf_path_pen::PdfPathPen;
use crate::query_cache::QueryCache;

/// A `skrifa`-backed loader for classic PostScript Type 1 programs.
pub struct Type1FontDriver;

impl Type1FontDriver {
    /// Creates a Type 1 driver.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for Type1FontDriver {
    /// Creates a Type 1 driver.
    fn default() -> Self {
        Self::new()
    }
}

impl FontDriver for Type1FontDriver {
    /// Reports whether this driver handles a classic Type 1 program.
    ///
    /// CFF and CID-keyed CFF programs have separate format tags and are
    /// intentionally not accepted here, even though they are also commonly
    /// used for PDF Type 1 font resources.
    fn supports(&self, format: FontProgramFormat) -> bool {
        matches!(format, FontProgramFormat::Type1)
    }

    /// Parses one in-memory PFA or PFB program into an immutable font face.
    ///
    /// Type 1 programs are standalone files rather than collections, so only
    /// face index zero is meaningful. Parsing is performed during loading so
    /// malformed encryption, dictionaries, or charstrings fail before the
    /// face enters the registry.
    fn load(&self, request: &FontLoadRequest) -> Result<Arc<dyn FontFace>, FontError> {
        let FontSource::Memory {
            data,
            format,
            face_index,
        } = &request.source
        else {
            return Err(FontError::InvalidProgram {
                format: Option::<FontProgramFormat>::from(&request.source)
                    .unwrap_or(FontProgramFormat::Type1),
                message: "the Type 1 driver requires an in-memory font program".into(),
            });
        };

        if !self.supports(*format) {
            return Err(FontError::DriverUnavailable { format: *format });
        }
        if *face_index != 0 {
            return Err(FontError::MissingFace {
                face_index: *face_index,
            });
        }

        Ok(Arc::new(Type1FontFace::new(
            data,
            *format,
            request.face_id,
            request.metadata_hint.clone(),
        )?))
    }
}

/// An immutable classic Type 1 face loaded by [`Type1FontDriver`].
pub struct Type1FontFace {
    /// Identity assigned by the driver that loaded this face.
    id: FontFaceId,
    /// Owned parsed Type 1 program, including decrypted charstrings and
    /// subroutines.
    font: ClassicType1Program,
    /// PDF- or application-provided metadata used for font matching.
    metadata: FontMetadata,
    /// Global metrics computed once while loading.
    metrics: Option<FontMetrics>,
    /// Lazily cached PostScript-name results, including lookup misses.
    glyphs_by_name: QueryCache<GlyphName, Option<GlyphId>>,
    /// Successfully evaluated charstring advances.
    advances: QueryCache<GlyphId, Option<f32>>,
}

impl Type1FontFace {
    /// Parses a classic Type 1 program and initializes its lazy query caches.
    fn new(
        data: &[u8],
        format: FontProgramFormat,
        id: FontFaceId,
        metadata: FontMetadata,
    ) -> Result<Self, FontError> {
        // Unlike sfnt and CFF references, `Type1Font` owns its decrypted
        // dictionaries and charstrings, so this face does not need a
        // self-referential owner.
        let font = ClassicType1Program::new(data).map_err(|error| FontError::InvalidProgram {
            format,
            message: error.to_string(),
        })?;
        let metrics = type1_metrics(&font);
        Ok(Self {
            id,
            font,
            metadata,
            metrics,
            glyphs_by_name: QueryCache::new(),
            advances: QueryCache::new(),
        })
    }

    /// Resolves and caches one PostScript glyph name.
    fn resolve_glyph_name(&self, name: &GlyphName) -> Option<GlyphId> {
        if let Some(glyph) = self.glyphs_by_name.get(name) {
            return glyph;
        }

        let glyph = self.font.glyph_names().find_map(|(glyph, candidate)| {
            (candidate.as_bytes() == name.0.as_ref()).then_some(GlyphId(glyph.to_u32()))
        });
        // Negative caching avoids rescanning malformed or incomplete Type 1
        // encodings for every text-showing operator.
        self.glyphs_by_name.insert(name.clone(), glyph);
        glyph
    }

    /// Evaluates a Type 1 charstring solely to obtain its horizontal advance.
    fn evaluate_horizontal_advance(&self, glyph: GlyphId) -> Result<Option<f32>, FontError> {
        let raw_glyph = skrifa::GlyphId::new(glyph.0);
        if raw_glyph.to_u32() >= self.font.num_glyphs() {
            return Err(FontError::MissingGlyph {
                face_id: self.id,
                glyph_id: glyph,
            });
        }
        self.font
            .draw(raw_glyph, None, &mut NullPen)
            .map_err(|error| FontError::InvalidProgram {
                format: FontProgramFormat::Type1,
                message: error.to_string(),
            })
    }
}

impl FontFace for Type1FontFace {
    /// Returns the stable identity assigned when the face was loaded.
    fn id(&self) -> FontFaceId {
        self.id
    }

    /// Returns the metadata associated with the PDF font request.
    fn metadata(&self) -> &FontMetadata {
        &self.metadata
    }

    /// Returns the face-wide design metrics, if the Type 1 UPEM fits the
    /// engine's metric representation.
    ///
    /// Type 1 exposes a font bounding box and a normalized units-per-em value.
    /// The bounding-box extremes are used as ascender and descender. Invalid
    /// or unrepresentable UPEM values make the optional metrics unavailable.
    fn metrics(&self) -> Option<FontMetrics> {
        self.metrics
    }

    /// Resolves a Unicode scalar through the parser's Adobe Glyph List map.
    ///
    /// Fonts without an AGL-resolvable glyph name return `None`; callers can
    /// still resolve those glyphs through PDF encodings or `glyph_for_name`.
    fn glyph_for_char(&self, character: char) -> Option<GlyphId> {
        // The parser builds this map from Adobe Glyph List names when its AGL
        // feature is enabled. Variant names follow the parser's fallback
        // behavior, which is preferable to duplicating AGL rules here.
        self.font
            .unicode_charmap()
            .map(character)
            .map(|glyph| GlyphId(glyph.to_u32()))
    }

    /// Resolves a normalized PDF/PostScript glyph name to its Type 1 glyph ID.
    ///
    /// The result is cached after its first scan without exposing the parser's
    /// internal charstring index.
    fn glyph_for_name(&self, name: &GlyphName) -> Option<GlyphId> {
        self.resolve_glyph_name(name)
    }

    /// Evaluates a charstring without retaining its outline.
    fn horizontal_advance(&self, glyph: GlyphId) -> Result<Option<f32>, FontError> {
        if let Some(advance) = self.advances.get(&glyph) {
            return Ok(advance);
        }

        let advance = self.evaluate_horizontal_advance(glyph)?;
        // Cache only successful evaluation. A malformed charstring must keep
        // returning its precise parsing error rather than becoming a miss.
        self.advances.insert(glyph, advance);
        Ok(advance)
    }

    /// Evaluates one charstring into a backend-neutral scalable outline.
    ///
    /// The Type 1 parser applies the font matrix while drawing, so the returned
    /// path is already in the face's normalized design coordinate system.
    fn glyph_geometry(
        &self,
        glyph: GlyphId,
        _pixels_per_em: f32,
    ) -> Result<Option<GlyphGeometry>, FontError> {
        // A scalable Type 1 outline is independent of requested pixel size;
        // the renderer applies the eventual text/device scale.
        let raw_glyph = skrifa::GlyphId::new(glyph.0);
        if raw_glyph.to_u32() >= self.font.num_glyphs() {
            return Ok(None);
        }

        let mut pen = PdfPathPen::default();
        self.font
            .draw(raw_glyph, None, &mut pen)
            .map_err(|error| FontError::InvalidProgram {
                format: FontProgramFormat::Type1,
                message: error.to_string(),
            })?;
        Ok(Some(GlyphGeometry::Outline(pen.into_path())))
    }
}

/// Extracts reusable global metrics from an owned Type 1 program.
fn type1_metrics(font: &ClassicType1Program) -> Option<FontMetrics> {
    // `upem` comes from the Type 1 FontMatrix and is normally 1000. Reject a
    // zero or unrepresentable value because it cannot define a layout scale.
    let units_per_em = u16::try_from(font.upem()).ok()?;
    (units_per_em != 0).then(|| FontMetrics {
        units_per_em,
        ascender: font.bbox().y_max.to_f32(),
        descender: font.bbox().y_min.to_f32(),
    })
}
