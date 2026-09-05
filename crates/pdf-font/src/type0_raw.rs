//! Standalone CID-CFF face implementation for PDF `CIDFontType0C` streams.
//!
//! Raw CFF has no OpenType cmap. Outlines are obtained by evaluating the
//! selected charstring with the CFF subfont chosen through `FDSelect`.

use read_fonts::{model::pen::NullPen, ps::cff::CffFontRef};
use self_cell::self_cell;

use crate::error::FontError;
use crate::font::{
    FontFace, FontFaceId, FontMetadata, FontMetrics, FontProgramFormat, GlyphGeometry, GlyphId,
    GlyphName,
};
use crate::pdf_path_pen::PdfPathPen;
use crate::query_cache::QueryCache;

use crate::type0::{CidGlyphMap, Type0FaceData, cff_glyph_id, validate_cid_cff};

/// Physical format reported in errors from the standalone CFF implementation.
const FORMAT: FontProgramFormat = FontProgramFormat::CidCff;

/// A standalone CID-keyed CFF face.
///
/// Public glyph identifiers are CIDs. They are translated through the CFF
/// charset before charstrings or subfont data are accessed.
pub(super) struct RawType0FontFace {
    /// Owned face data and reusable raw CFF parser view.
    program: RawCffProgram,
    /// Successfully evaluated glyph advances.
    advances: QueryCache<GlyphId, Option<f32>>,
}

impl RawType0FontFace {
    /// Parses and constructs a face whose CFF view shares owned bytes.
    pub(super) fn new(common: Type0FaceData) -> Result<Self, FontError> {
        let face_index = common.face_index;
        let program = RawCffProgram::try_new(common, move |common| {
            RawCffProgramRef::parse(&common.data, face_index)
        })?;
        Ok(Self {
            program,
            advances: QueryCache::new(),
        })
    }

    /// Selects the private dictionary and local subroutines for a physical GID.
    ///
    /// CID-keyed CFF may use `FDSelect` to associate glyphs with different font
    /// dictionaries and matrices. Geometry must use the selected subfont to
    /// remain in the correct design coordinate system.
    fn subfont(
        &self,
        cff: &CffFontRef<'_>,
        glyph: GlyphId,
        cff_glyph: read_fonts::types::GlyphId,
    ) -> Result<read_fonts::ps::cff::Subfont, FontError> {
        let index = cff
            .subfont_index(cff_glyph)
            .ok_or_else(|| FontError::MissingGlyph {
                face_id: self.id(),
                glyph_id: glyph,
            })?;
        cff.subfont(index, &[])
            .map_err(|error| FontError::InvalidProgram {
                format: FORMAT,
                message: error.to_string(),
            })
    }

    /// Evaluates a CID charstring solely to obtain its horizontal advance.
    fn evaluate_horizontal_advance(&self, glyph: GlyphId) -> Result<Option<f32>, FontError> {
        let program = self.program.borrow_dependent();
        let cff_glyph = cff_glyph_id(&program.glyphs, self.id(), glyph)?;
        let subfont = self.subfont(&program.cff, glyph, cff_glyph)?;
        program
            .cff
            .draw(&subfont, cff_glyph, &[], None, &mut NullPen)
            .map_err(|error| FontError::InvalidProgram {
                format: FORMAT,
                message: error.to_string(),
            })
    }
}

/// Parsed raw CFF data and load-time accelerators.
struct RawCffProgramRef<'a> {
    /// Parsed CFF top dictionary and charstring indexes.
    cff: CffFontRef<'a>,
    /// Global metrics computed from the CFF font matrix and bounds.
    metrics: Option<FontMetrics>,
    /// Retained charset with lazy public-CID lookup caches.
    glyphs: CidGlyphMap<'a>,
}

impl<'a> RawCffProgramRef<'a> {
    /// Parses and validates one standalone CID-keyed CFF top dictionary.
    fn parse(data: &'a [u8], face_index: u32) -> Result<Self, FontError> {
        let cff = parse_cff(data, face_index)?;
        validate_cid_cff(&cff, FORMAT)?;
        let metrics = raw_cff_metrics(&cff);
        let glyphs = CidGlyphMap::new(&cff, FORMAT)?;
        Ok(Self {
            cff,
            metrics,
            glyphs,
        })
    }
}

self_cell!(
    /// Keeps Type 0 face data and its borrowed raw CFF parser view together.
    ///
    /// [`CffFontRef`] borrows the program bytes stored in [`Type0FaceData`].
    /// The cell pins that owner so the parsed top dictionary and charset can
    /// be retained across glyph calls instead of reconstructed repeatedly.
    /// The owner is [`Type0FaceData`]; the dependent is [`RawCffProgramRef`].
    struct RawCffProgram {
        owner: Type0FaceData,

        #[covariant]
        dependent: RawCffProgramRef,
    }
);

impl FontFace for RawType0FontFace {
    /// Returns the identity allocated when this face was loaded.
    fn id(&self) -> FontFaceId {
        self.program.borrow_owner().id
    }

    /// Returns the PDF- or application-supplied matching metadata unchanged.
    fn metadata(&self) -> &FontMetadata {
        &self.program.borrow_owner().metadata
    }

    /// Returns global raw-CFF metrics in unscaled design units.
    ///
    /// Standalone CFF has no OpenType line metrics, so the declared font bounds
    /// supply the ascender and descender.
    fn metrics(&self) -> Option<FontMetrics> {
        self.program.borrow_dependent().metrics
    }

    /// Returns no Unicode mapping because raw CFF contains no Unicode cmap.
    fn glyph_for_char(&self, _character: char) -> Option<GlyphId> {
        None
    }

    /// Returns no name mapping because CID charsets contain numeric CIDs.
    fn glyph_for_name(&self, _name: &GlyphName) -> Option<GlyphId> {
        None
    }

    /// Evaluates a CID charstring without retaining its outline.
    fn horizontal_advance(&self, glyph: GlyphId) -> Result<Option<f32>, FontError> {
        if let Some(advance) = self.advances.get(&glyph) {
            return Ok(advance);
        }

        let advance = self.evaluate_horizontal_advance(glyph)?;
        // Do not cache parsing failures: subsequent calls must preserve the
        // original malformed-program error rather than report a missing width.
        self.advances.insert(glyph, advance);
        Ok(advance)
    }

    /// Evaluates a CID into an unscaled backend-neutral outline.
    fn glyph_geometry(
        &self,
        glyph: GlyphId,
        _pixels_per_em: f32,
    ) -> Result<Option<GlyphGeometry>, FontError> {
        let program = self.program.borrow_dependent();
        let cff_glyph = cff_glyph_id(&program.glyphs, self.id(), glyph)?;
        let subfont = self.subfont(&program.cff, glyph, cff_glyph)?;
        let mut pen = PdfPathPen::default();
        // No ppem scale is requested because the text engine retains vector
        // geometry in design units. CFF font and subfont matrices still apply.
        program
            .cff
            .draw(&subfont, cff_glyph, &[], None, &mut pen)
            .map_err(|error| FontError::InvalidProgram {
                format: FORMAT,
                message: error.to_string(),
            })?;
        Ok(Some(GlyphGeometry::Outline(pen.into_path())))
    }
}

/// Computes reusable global metrics for a standalone CFF program.
fn raw_cff_metrics(cff: &CffFontRef<'_>) -> Option<FontMetrics> {
    let units_per_em = u16::try_from(cff.upem()).ok()?;
    let bounds = cff.metadata()?.bbox();
    (units_per_em != 0).then_some(FontMetrics {
        units_per_em,
        ascender: bounds.y_max.to_f32(),
        descender: bounds.y_min.to_f32(),
    })
}

/// Parses a standalone CFF top dictionary.
///
/// A nonzero unavailable top-dictionary index is reported as a missing face,
/// while failure at index zero identifies an invalid program.
fn parse_cff(data: &[u8], face_index: u32) -> Result<CffFontRef<'_>, FontError> {
    CffFontRef::new_cff(data, face_index, None).map_err(|error| {
        if face_index == 0 {
            FontError::InvalidProgram {
                format: FORMAT,
                message: error.to_string(),
            }
        } else {
            FontError::MissingFace { face_index }
        }
    })
}
