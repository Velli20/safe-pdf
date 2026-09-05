//! TrueType and OpenType/TrueType font support backed by `skrifa`.
//!
//! Font bytes and their parsed views are retained together by each loaded face.
//! Metrics and outlines are requested unscaled because the text engine keeps
//! geometry in font design coordinates and applies sizing while laying out and
//! painting glyphs.

use std::sync::Arc;

use bytes::Bytes;
use read_fonts::{TableProvider, tables::post::DEFAULT_GLYPH_NAMES, types::Version16Dot16};
use self_cell::self_cell;
use skrifa::{
    FontRef, GlyphNames, MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlineGlyphCollection, OutlineGlyphFormat},
};

use crate::error::FontError;
use crate::font::{
    FontDriver, FontFace, FontFaceId, FontLoadRequest, FontMetadata, FontMetrics,
    FontProgramFormat, FontSource, GlyphGeometry, GlyphId, GlyphName,
};
use crate::pdf_path_pen::PdfPathPen;
use crate::query_cache::QueryCache;

/// Borrowed parser state retained by an owning [`TrueTypeProgram`].
struct TrueTypeProgramRef<'a> {
    /// Selected standalone font or collection member.
    font: FontRef<'a>,
    /// PostScript name provider reused by lazy name lookups.
    glyph_names: Option<GlyphNames<'a>>,
    /// Outline provider reused by page-scoped glyph geometry caching.
    outlines: OutlineGlyphCollection<'a>,
}

impl<'a> TrueTypeProgramRef<'a> {
    /// Parses one face and prepares all table providers used by rendering.
    fn parse(
        data: &'a [u8],
        face_index: u32,
        format: FontProgramFormat,
    ) -> Result<Self, FontError> {
        let font = parse_font(data, face_index, format)?;
        let outlines = font.outline_glyphs();
        validate_outline_format(&outlines, format)?;

        let glyph_names = usable_glyph_names(&font);
        Ok(Self {
            font,
            glyph_names,
            outlines,
        })
    }
}

self_cell!(
    /// Keeps immutable font bytes and parser views borrowing those bytes alive together.
    ///
    /// `skrifa` providers borrow their source buffer, while a loaded face must
    /// own that buffer and move freely behind `Arc<dyn FontFace>`. `self_cell`
    /// pins the owner and exposes only lifetime-safe dependent borrows. This
    /// lets the driver parse once during loading without project-local unsafe
    /// code or reparsing the container for every glyph query.
    /// The cell's owner is the immutable [`Bytes`] buffer and its dependent is
    /// [`TrueTypeProgramRef`], which contains the borrowed parser providers.
    struct TrueTypeProgram {
        owner: Bytes,

        #[covariant]
        dependent: TrueTypeProgramRef,
    }
);

/// A `skrifa`-backed loader for TrueType outlines.
///
/// The driver accepts standalone TrueType programs, TrueType collections, and
/// OpenType containers whose outlines are stored in `glyf` or `VARC` tables.
pub struct TrueTypeFontDriver;

impl TrueTypeFontDriver {
    /// Creates a TrueType driver.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for TrueTypeFontDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl FontDriver for TrueTypeFontDriver {
    fn supports(&self, format: FontProgramFormat) -> bool {
        matches!(
            format,
            FontProgramFormat::TrueType
                | FontProgramFormat::OpenTypeTrueType
                | FontProgramFormat::OpenTypeCff
        )
    }

    fn load(&self, request: &FontLoadRequest) -> Result<Arc<dyn FontFace>, FontError> {
        let FontSource::Memory {
            data,
            format,
            face_index,
        } = &request.source
        else {
            return Err(FontError::InvalidProgram {
                format: Option::<FontProgramFormat>::from(&request.source)
                    .unwrap_or(FontProgramFormat::TrueType),
                message: "the TrueType driver requires an in-memory font program".into(),
            });
        };

        if !self.supports(*format) {
            return Err(FontError::DriverUnavailable { format: *format });
        }

        Ok(Arc::new(TrueTypeFontFace::new(
            data.clone(),
            *format,
            *face_index,
            request.face_id,
            request.metadata_hint.clone(),
        )?))
    }
}

/// An immutable TrueType face loaded by [`TrueTypeFontDriver`].
pub struct TrueTypeFontFace {
    /// Identity assigned by the driver that loaded this face.
    id: FontFaceId,
    /// Shared backing storage and parser views created once during loading.
    program: TrueTypeProgram,
    /// Declared container format used when reporting parsing failures.
    format: FontProgramFormat,
    /// PDF- or application-provided metadata used for font matching.
    metadata: FontMetadata,
    /// Global design-space metrics computed during loading.
    metrics: Option<FontMetrics>,
    /// Requested PostScript names cached after their first lookup.
    glyphs_by_name: QueryCache<GlyphName, Option<GlyphId>>,
}

impl TrueTypeFontFace {
    /// Creates a face and parses its retained program exactly once.
    fn new(
        data: Bytes,
        format: FontProgramFormat,
        face_index: u32,
        id: FontFaceId,
        metadata: FontMetadata,
    ) -> Result<Self, FontError> {
        let program = TrueTypeProgram::try_new(data, move |data| {
            TrueTypeProgramRef::parse(data, face_index, format)
        })?;
        let metrics = font_metrics(&program.borrow_dependent().font);
        Ok(Self {
            id,
            program,
            format,
            metadata,
            metrics,
            glyphs_by_name: QueryCache::new(),
        })
    }

    /// Converts the engine's public glyph identifier to `skrifa`'s type.
    fn skrifa_glyph_id(&self, glyph: GlyphId) -> skrifa::GlyphId {
        skrifa::GlyphId::new(glyph.0)
    }

    /// Resolves and caches one PostScript glyph name.
    fn resolve_glyph_name(&self, name: &GlyphName) -> Option<GlyphId> {
        if let Some(glyph) = self.glyphs_by_name.get(name) {
            return glyph;
        }

        let glyph = self.find_glyph_name(name);
        // Cache misses as well as hits because malformed PDFs often repeat the
        // same unavailable encoding name across multiple text operators.
        self.glyphs_by_name.insert(name.clone(), glyph);
        glyph
    }

    /// Scans the retained name provider without reparsing the font container.
    fn find_glyph_name(&self, name: &GlyphName) -> Option<GlyphId> {
        self.program
            .borrow_dependent()
            .glyph_names
            .as_ref()?
            .iter()
            .find_map(|(glyph, candidate)| {
                // `skrifa` invents `gidNNN` names for fonts without real
                // PostScript names. Synthetic names are not valid PDF name
                // mappings and must remain lookup misses.
                (!candidate.is_synthesized() && candidate.as_str().as_bytes() == name.0.as_ref())
                    .then_some(GlyphId(glyph.to_u32()))
            })
    }
}

impl FontFace for TrueTypeFontFace {
    fn id(&self) -> FontFaceId {
        self.id
    }

    fn metadata(&self) -> &FontMetadata {
        &self.metadata
    }

    fn metrics(&self) -> Option<FontMetrics> {
        self.metrics
    }

    fn glyph_for_char(&self, character: char) -> Option<GlyphId> {
        self.program
            .borrow_dependent()
            .font
            .charmap()
            .map(character)
            .map(|glyph| GlyphId(glyph.to_u32()))
    }

    fn glyph_for_name(&self, name: &GlyphName) -> Option<GlyphId> {
        self.resolve_glyph_name(name)
    }

    fn horizontal_advance(&self, glyph: GlyphId) -> Result<Option<f32>, FontError> {
        Ok(self
            .program
            .borrow_dependent()
            .font
            .glyph_metrics(Size::unscaled(), LocationRef::default())
            .advance_width(self.skrifa_glyph_id(glyph)))
    }

    fn glyph_geometry(
        &self,
        glyph: GlyphId,
        _pixels_per_em: f32,
    ) -> Result<Option<GlyphGeometry>, FontError> {
        let Some(outline) = self
            .program
            .borrow_dependent()
            .outlines
            .get(self.skrifa_glyph_id(glyph))
        else {
            return Ok(None);
        };

        let mut pen = PdfPathPen::default();
        // Vector geometry stays in design coordinates. `pixels_per_em` is
        // relevant to bitmap strikes, not to this scalable outline path.
        outline
            .draw(
                DrawSettings::from((Size::unscaled(), LocationRef::default())),
                &mut pen,
            )
            .map_err(|error| FontError::InvalidProgram {
                format: self.format,
                message: error.to_string(),
            })?;
        Ok(Some(GlyphGeometry::Outline(pen.into_path())))
    }
}

/// Creates a name provider only when an optional version 2 `post` table is structurally usable.
///
/// `skrifa` currently assumes that every version 2 table contains its variable fields. Some PDF
/// producers embed otherwise valid fonts with only the fixed `post` header, so checking those
/// fields first keeps missing glyph-name metadata from invalidating the font's outlines.
fn usable_glyph_names<'a>(font: &FontRef<'a>) -> Option<GlyphNames<'a>> {
    let Ok(post) = font.post() else {
        return Some(font.glyph_names());
    };
    if post.version() != Version16Dot16::VERSION_2_0 {
        return Some(font.glyph_names());
    }

    let num_glyphs = post.num_glyphs()?;
    if num_glyphs == 0 {
        return Some(font.glyph_names());
    }
    let name_indices = post.glyph_name_index()?;
    let has_custom_names = name_indices
        .iter()
        .any(|index| usize::from(index.get()) >= DEFAULT_GLYPH_NAMES.len());
    if has_custom_names && post.string_data().is_none() {
        return None;
    }
    Some(font.glyph_names())
}

/// Computes size-independent metrics once for reuse by all layout runs.
fn font_metrics(font: &FontRef<'_>) -> Option<FontMetrics> {
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    (metrics.units_per_em != 0).then_some(FontMetrics {
        units_per_em: metrics.units_per_em,
        ascender: metrics.ascent,
        descender: metrics.descent,
    })
}

/// Verifies that the parsed outline tables agree with the declared format.
fn validate_outline_format(
    outlines: &OutlineGlyphCollection<'_>,
    format: FontProgramFormat,
) -> Result<(), FontError> {
    let valid = match format {
        FontProgramFormat::OpenTypeCff => matches!(
            outlines.format(),
            Some(OutlineGlyphFormat::Cff | OutlineGlyphFormat::Cff2)
        ),
        _ => matches!(
            outlines.format(),
            Some(OutlineGlyphFormat::Glyf | OutlineGlyphFormat::Varc)
        ),
    };
    if valid {
        return Ok(());
    }
    Err(FontError::InvalidProgram {
        format,
        message: "the OpenType outline format does not match the declared source".into(),
    })
}

/// Parses either a standalone font or one member of a font collection.
fn parse_font(
    data: &[u8],
    face_index: u32,
    format: FontProgramFormat,
) -> Result<FontRef<'_>, FontError> {
    // `from_index` handles both single fonts and TTC containers. For a
    // nonzero requested index, failure is exposed as a missing collection
    // member rather than as corruption of the entire program.
    FontRef::from_index(data, face_index).map_err(|error| {
        if face_index == 0 {
            FontError::InvalidProgram {
                format,
                message: error.to_string(),
            }
        } else {
            FontError::MissingFace { face_index }
        }
    })
}
