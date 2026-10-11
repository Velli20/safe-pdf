//! Bare Compact Font Format (CFF) support backed by `read-fonts`.
//!
//! A bare CFF program, such as PDF `/Type1C` data, has no OpenType tables:
//! glyph names come from the CFF charset, advances from each charstring, and
//! Unicode lookup from the Adobe Glyph List applied to those names. CID-keyed
//! programs are accepted too; their charset maps CIDs, so they have no names. Font bytes and the parser view
//! borrowing them are retained together by each loaded face.

use std::sync::Arc;

use bytes::Bytes;
use read_fonts::{
    model::pen::NullPen,
    ps::{
        cff::{CffFontRef, Subfont},
        charmap::Charmap,
    },
};
use self_cell::self_cell;

use crate::error::FontError;
use crate::font::{
    FontDriver, FontFace, FontFaceId, FontLoadRequest, FontMetadata, FontMetrics,
    FontProgramFormat, FontSource, GlyphGeometry, GlyphId, GlyphName,
};
use crate::pdf_path_pen::PdfPathPen;
use crate::query_cache::QueryCache;

/// Borrowed parser state retained by an owning [`CffProgram`].
struct CffProgramRef<'a> {
    /// Parsed top-level font.
    font: CffFontRef<'a>,
    /// Private dictionaries, one for a name-keyed font or one per FDArray entry.
    subfonts: Vec<Subfont>,
}

impl<'a> CffProgramRef<'a> {
    /// Parses a program and the private dictionaries its charstrings use.
    fn parse(data: &'a [u8]) -> Result<Self, FontError> {
        let font =
            CffFontRef::new_cff(data, 0, None).map_err(|error| FontError::InvalidProgram {
                format: FontProgramFormat::Cff,
                message: error.to_string(),
            })?;
        let subfonts = (0..font.num_subfonts())
            .map(|index| font.subfont(index, &[]))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| FontError::InvalidProgram {
                format: FontProgramFormat::Cff,
                message: error.to_string(),
            })?;
        Ok(Self { font, subfonts })
    }

    /// Iterates the charset's glyph names that are valid UTF-8.
    ///
    /// A CID-keyed charset maps CIDs rather than string identifiers, so it yields no names.
    fn glyph_names(&self) -> impl Iterator<Item = (read_fonts::types::GlyphId, &'a str)> + '_ {
        let charset = (!self.font.is_cid()).then(|| self.font.charset()).flatten();
        charset.into_iter().flat_map(|charset| {
            charset.iter().filter_map(|(glyph, sid)| {
                let name = self.font.string(sid)?;
                Some((glyph, std::str::from_utf8(name).ok()?))
            })
        })
    }

    /// Computes face-wide design metrics from the font matrix and bounding box.
    fn metrics(&self) -> Option<FontMetrics> {
        // `upem` comes from the top-level FontMatrix and is normally 1000. Reject a
        // zero or unrepresentable value because it cannot define a layout scale.
        let units_per_em = u16::try_from(self.font.upem()).ok()?;
        let bbox = self.font.metadata()?.bbox();
        (units_per_em != 0).then(|| FontMetrics {
            units_per_em,
            ascender: bbox.y_max.to_f32(),
            descender: bbox.y_min.to_f32(),
        })
    }

    /// Evaluates one charstring into `pen`, returning its advance in design units.
    fn draw(
        &self,
        glyph: GlyphId,
        pen: &mut impl read_fonts::model::pen::OutlinePen,
    ) -> Result<Option<f32>, FontError> {
        let raw_glyph = read_fonts::types::GlyphId::new(glyph.0);
        let subfont = self
            .font
            .subfont_index(raw_glyph)
            .and_then(|index| self.subfonts.get(usize::from(index)))
            .ok_or_else(|| FontError::InvalidProgram {
                format: FontProgramFormat::Cff,
                message: format!("glyph {} has no private dictionary", glyph.0),
            })?;
        self.font
            .draw(subfont, raw_glyph, &[], None, pen)
            .map_err(|error| FontError::InvalidProgram {
                format: FontProgramFormat::Cff,
                message: error.to_string(),
            })
    }
}

self_cell!(
    /// Keeps immutable CFF bytes and the parser view borrowing them alive together.
    ///
    /// The cell's owner is the immutable [`Bytes`] buffer and its dependent is
    /// [`CffProgramRef`], so the program is parsed once while loading.
    struct CffProgram {
        owner: Bytes,

        #[covariant]
        dependent: CffProgramRef,
    }
);

/// A `read-fonts`-backed loader for bare CFF programs.
pub struct CffFontDriver;

impl CffFontDriver {
    /// Creates a CFF driver.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for CffFontDriver {
    /// Creates a CFF driver.
    fn default() -> Self {
        Self::new()
    }
}

impl FontDriver for CffFontDriver {
    /// Reports whether this driver handles a bare CFF program.
    fn supports(&self, format: FontProgramFormat) -> bool {
        matches!(format, FontProgramFormat::Cff)
    }

    /// Parses one in-memory CFF program into an immutable font face.
    ///
    /// Only the first top-level font of a CFF font set is loaded, so only
    /// face index zero is meaningful.
    fn load(&self, request: &FontLoadRequest) -> Result<Arc<dyn FontFace>, FontError> {
        let FontSource::Memory {
            data,
            format,
            face_index,
        } = &request.source
        else {
            return Err(FontError::InvalidProgram {
                format: Option::<FontProgramFormat>::from(&request.source)
                    .unwrap_or(FontProgramFormat::Cff),
                message: "the CFF driver requires an in-memory font program".into(),
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

        Ok(Arc::new(CffFontFace::new(
            data.clone(),
            request.face_id,
            request.metadata_hint.clone(),
        )?))
    }
}

/// An immutable bare CFF face loaded by [`CffFontDriver`].
pub struct CffFontFace {
    /// Identity assigned by the driver that loaded this face.
    id: FontFaceId,
    /// Shared backing storage and the parser view created once during loading.
    program: CffProgram,
    /// Unicode lookup built from the charset's Adobe Glyph List names.
    unicode_charmap: Charmap,
    /// PDF- or application-provided metadata used for font matching.
    metadata: FontMetadata,
    /// Global design-space metrics computed during loading.
    metrics: Option<FontMetrics>,
    /// Requested PostScript names cached after their first lookup.
    glyphs_by_name: QueryCache<GlyphName, Option<GlyphId>>,
    /// Successfully evaluated charstring advances.
    advances: QueryCache<GlyphId, Option<f32>>,
}

impl CffFontFace {
    /// Creates a face and parses its retained program exactly once.
    fn new(data: Bytes, id: FontFaceId, metadata: FontMetadata) -> Result<Self, FontError> {
        let program = CffProgram::try_new(data, |data| CffProgramRef::parse(data))?;
        let unicode_charmap = Charmap::from_glyph_names(program.borrow_dependent().glyph_names());
        let metrics = program.borrow_dependent().metrics();
        Ok(Self {
            id,
            program,
            unicode_charmap,
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

        let glyph = self
            .program
            .borrow_dependent()
            .glyph_names()
            .find_map(|(glyph, candidate)| {
                (candidate.as_bytes() == name.0.as_ref()).then_some(GlyphId(glyph.to_u32()))
            });
        // Negative caching avoids rescanning the charset for names the font lacks.
        self.glyphs_by_name.insert(name.clone(), glyph);
        glyph
    }

    /// Fails with [`FontError::MissingGlyph`] for an identifier outside the charstrings.
    fn check_glyph(&self, glyph: GlyphId) -> Result<(), FontError> {
        if glyph.0 < self.program.borrow_dependent().font.num_glyphs() {
            Ok(())
        } else {
            Err(FontError::MissingGlyph {
                face_id: self.id,
                glyph_id: glyph,
            })
        }
    }
}

impl FontFace for CffFontFace {
    /// Returns the stable identity assigned when the face was loaded.
    fn id(&self) -> FontFaceId {
        self.id
    }

    /// Returns the metadata associated with the PDF font request.
    fn metadata(&self) -> &FontMetadata {
        &self.metadata
    }

    /// Returns the face-wide design metrics taken from the font bounding box.
    fn metrics(&self) -> Option<FontMetrics> {
        self.metrics
    }

    /// Resolves a Unicode scalar through the charset's Adobe Glyph List names.
    fn glyph_for_char(&self, character: char) -> Option<GlyphId> {
        self.unicode_charmap
            .map(character)
            .map(|glyph| GlyphId(glyph.to_u32()))
    }

    /// Resolves a PDF/PostScript glyph name to its CFF glyph ID.
    fn glyph_for_name(&self, name: &GlyphName) -> Option<GlyphId> {
        self.resolve_glyph_name(name)
    }

    /// Evaluates a charstring without retaining its outline.
    fn horizontal_advance(&self, glyph: GlyphId) -> Result<Option<f32>, FontError> {
        if let Some(advance) = self.advances.get(&glyph) {
            return Ok(advance);
        }

        self.check_glyph(glyph)?;
        let advance = self.program.borrow_dependent().draw(glyph, &mut NullPen)?;
        // Cache only successful evaluation so a malformed charstring keeps its error.
        self.advances.insert(glyph, advance);
        Ok(advance)
    }

    /// Evaluates one charstring into a backend-neutral scalable outline.
    fn glyph_geometry(
        &self,
        glyph: GlyphId,
        _pixels_per_em: f32,
    ) -> Result<Option<GlyphGeometry>, FontError> {
        if self.check_glyph(glyph).is_err() {
            return Ok(None);
        }

        let mut pen = PdfPathPen::default();
        self.program.borrow_dependent().draw(glyph, &mut pen)?;
        Ok(Some(GlyphGeometry::Outline(pen.into_path())))
    }
}
