//! OpenType/CFF face implementation for PDF `CIDFontType0` descendants.
//!
//! The OpenType view supplies Unicode cmap lookup, global metrics, and
//! high-level outlines. A raw CFF view of the same table is retained for
//! the CID charset that translates public CIDs into physical GIDs.

use std::num::NonZeroU16;

use read_fonts::{FontRef, TableProvider, ps::cff::CffFontRef};
use self_cell::self_cell;
use skrifa::{
    MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlineGlyphCollection, OutlineGlyphFormat},
};

use crate::error::FontError;
use crate::font::{
    FontFace, FontFaceId, FontMetrics, FontProgramFormat, GlyphGeometry, GlyphId, GlyphName,
    LineMetrics,
};
use crate::pdf_path_pen::PdfPathPen;

use crate::type0::{CidGlyphMap, Type0FaceData, cff_glyph_id, validate_cid_cff};

/// Physical format reported in errors from the OpenType implementation.
const FORMAT: FontProgramFormat = FontProgramFormat::OpenTypeCff;

/// An OpenType face containing CID-keyed CFF1 outlines.
pub(super) struct OpenTypeType0FontFace {
    /// Owned face data and reusable parser views.
    program: OpenTypeProgram,
}

impl OpenTypeType0FontFace {
    /// Parses and constructs a face whose borrowed views share owned bytes.
    pub(super) fn new(common: Type0FaceData) -> Result<Self, FontError> {
        let face_index = common.face_index;
        let program = OpenTypeProgram::try_new(common, move |common| {
            OpenTypeProgramRef::parse(&common.data, face_index)
        })?;
        Ok(Self { program })
    }
}

impl FontFace for OpenTypeType0FontFace {
    /// Returns the identity allocated when this face was loaded.
    fn id(&self) -> FontFaceId {
        self.program.borrow_owner().id
    }

    /// Returns OpenType scale and vertical metrics in unscaled design units.
    fn metrics(&self) -> Option<FontMetrics> {
        self.program.borrow_dependent().metrics
    }

    /// Resolves Unicode through the OpenType cmap and returns the matching CID.
    ///
    /// The cmap produces a physical GID. Reversing that GID through the CFF
    /// charset preserves the Type 0 face invariant that public glyph IDs are
    /// CIDs rather than charstring indices.
    fn glyph_for_char(&self, character: char) -> Option<GlyphId> {
        let program = self.program.borrow_dependent();
        let physical_glyph = program.font.charmap().map(character)?;
        program.glyphs.cid(physical_glyph)
    }

    /// Returns no name mapping because CID charsets contain numeric CIDs.
    fn glyph_for_name(&self, _name: &GlyphName) -> Option<GlyphId> {
        None
    }

    /// Reads the unscaled horizontal advance after resolving the CID to a physical GID.
    fn horizontal_advance(&self, glyph: GlyphId) -> Result<Option<f32>, FontError> {
        let program = self.program.borrow_dependent();
        let cff_glyph = cff_glyph_id(&program.glyphs, self.id(), glyph)?;
        Ok(program
            .font
            .glyph_metrics(Size::unscaled(), LocationRef::default())
            .advance_width(cff_glyph))
    }

    /// Draws an unscaled OpenType/CFF outline into backend-neutral geometry.
    fn glyph_geometry(&self, glyph: GlyphId) -> Result<Option<GlyphGeometry>, FontError> {
        let program = self.program.borrow_dependent();
        // A CID outside the charset has no charstring, so it draws nothing.
        let Ok(cff_glyph) = cff_glyph_id(&program.glyphs, self.id(), glyph) else {
            return Ok(None);
        };
        let Some(outline) = program.font.outline_glyphs().get(cff_glyph) else {
            return Ok(None);
        };

        let mut pen = PdfPathPen::default();
        // Keep vector geometry in design coordinates at the default variation
        // location. Text sizing and device transforms are applied later.
        outline
            .draw(
                DrawSettings::from((Size::unscaled(), LocationRef::default())),
                &mut pen,
            )
            .map_err(|error| FontError::InvalidProgram {
                format: FORMAT,
                message: error.to_string(),
            })?;
        Ok(Some(GlyphGeometry::Outline(pen.into_path())))
    }
}

/// Borrowed views of one selected OpenType/CFF collection member.
struct OpenTypeProgramRef<'a> {
    /// Selected standalone OpenType font or collection member.
    font: FontRef<'a>,
    /// Global design-space metrics computed during loading.
    metrics: Option<FontMetrics>,
    /// Retained CFF charset with lazy CID/GID lookup caches.
    glyphs: CidGlyphMap<'a>,
}

impl<'a> OpenTypeProgramRef<'a> {
    /// Parses and validates one CID-keyed OpenType/CFF collection member.
    fn parse(data: &'a [u8], face_index: u32) -> Result<Self, FontError> {
        let font = parse_font(data, face_index)?;
        let cff = parse_cff(&font)?;
        validate_cid_cff(&cff, FORMAT)?;

        let outlines = font.outline_glyphs();
        validate_outline_format(&outlines)?;

        Ok(Self {
            metrics: open_type_metrics(&font, &cff),
            glyphs: CidGlyphMap::new(&cff, FORMAT)?,
            font,
        })
    }
}

self_cell!(
    /// Keeps Type 0 face data and its borrowed OpenType/CFF providers together.
    ///
    /// OpenType table providers borrow the program buffer stored in
    /// [`Type0FaceData`]. `self_cell` pins that owner and prevents the borrowed
    /// [`OpenTypeProgram`] from escaping it, allowing one load-time parse while
    /// retaining `Send + Sync` for the enclosing font face. The owner is
    /// [`Type0FaceData`]; the dependent is [`OpenTypeProgramRef`].
    struct OpenTypeProgram {
        owner: Type0FaceData,

        #[covariant]
        dependent: OpenTypeProgramRef,
    }
);

/// Parses either an OpenType font or one member of an OpenType collection.
fn parse_font(data: &[u8], face_index: u32) -> Result<FontRef<'_>, FontError> {
    // `from_index` handles a standalone sfnt and TTC/OTC collections with the
    // same interface.
    FontRef::from_index(data, face_index).map_err(|error| {
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

/// Parses the CFF1 table using the OpenType head table's design-space scale.
fn parse_cff<'a>(font: &FontRef<'a>) -> Result<CffFontRef<'a>, FontError> {
    let cff_table = font.cff().map_err(|error| FontError::InvalidProgram {
        format: FORMAT,
        message: error.to_string(),
    })?;
    // OpenType `head` is the authoritative UPEM. Passing it to the raw CFF
    // view keeps FontMatrix and subfont transforms in the same coordinate
    // system as the high-level metrics provider.
    let units_per_em = font.head().ok().map(|head| i32::from(head.units_per_em()));
    CffFontRef::new_cff(cff_table.offset_data().as_bytes(), 0, units_per_em).map_err(|error| {
        FontError::InvalidProgram {
            format: FORMAT,
            message: error.to_string(),
        }
    })
}

/// Rejects CFF2 and incorrectly tagged TrueType outline containers.
fn validate_outline_format(outlines: &OutlineGlyphCollection<'_>) -> Result<(), FontError> {
    // CFF2 has no CFF1 charset, and incorrectly tagged TrueType outlines must
    // not pass merely because their OpenType container is readable.
    if matches!(outlines.format(), Some(OutlineGlyphFormat::Cff)) {
        return Ok(());
    }
    Err(FontError::InvalidProgram {
        format: FORMAT,
        message: "the font does not contain CFF1 outlines".into(),
    })
}

/// Computes reusable OpenType metrics, taking the line extent from the typographic
/// metrics, else the font bounds, else the CFF bounding box.
fn open_type_metrics(font: &FontRef<'_>, cff: &CffFontRef<'_>) -> Option<FontMetrics> {
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    let units_per_em = NonZeroU16::new(metrics.units_per_em)?;
    let line = LineMetrics::from_design_units(metrics.ascent, metrics.descent, units_per_em)
        .or_else(|| {
            metrics.bounds.and_then(|bounds| {
                LineMetrics::from_design_units(bounds.y_max, bounds.y_min, units_per_em)
            })
        })
        .or_else(|| {
            let bounds = cff.metadata()?.bbox();
            LineMetrics::from_design_units(
                bounds.y_max.to_f32(),
                bounds.y_min.to_f32(),
                units_per_em,
            )
        });
    Some(FontMetrics { units_per_em, line })
}
