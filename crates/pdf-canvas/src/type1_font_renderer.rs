use crate::canvas_backend::CanvasBackend;
use crate::font_renderer_support::{glyph_base_transform, normalized_units_per_em};
use crate::pdf_canvas::PdfCanvas;
use crate::pdf_path_pen::PdfPathPen;
use crate::{error::PdfCanvasError, text_renderer::TextRenderer};
use pdf_font::type1_font::Type1FontProgramFormat;
use pdf_graphics::transform::Transform;
use read_fonts::TableProvider;
use read_fonts::ps::cff::CffFontRef;
use read_fonts::ps::cff::charset::Charset as CffCharset;
use read_fonts::ps::string::Sid;
use read_fonts::ps::type1::Type1Font as ClassicType1Font;
use skrifa::outline::{OutlineGlyph, OutlineGlyphCollection};
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// The parsed Type 1 font program, specialized by file format.
enum Type1RendererFont<'a> {
    /// An OpenType CFF font program.
    OpenTypeCff {
        /// The parsed font reference.
        font_ref: FontRef<'a>,
        /// The outline collection extracted from the font.
        outlines: OutlineGlyphCollection<'a>,
        /// CID-to-glyph mapping for CID-keyed CFF fonts.
        cid_charset: Option<CffCharset<'a>>,
    },
    /// A classic Type 1 font program.
    ClassicType1(ClassicType1Font),
}

struct ParsedType1Font<'a> {
    font: Type1RendererFont<'a>,
    units_per_em: u16,
}

/// The rendering state shared by both Type 1 font formats.
pub(crate) struct Type1FontRenderer<'a, 'b, B: CanvasBackend> {
    /// The canvas where glyphs are rendered.
    canvas: &'b mut PdfCanvas<'a, B>,
    /// The parsed Type 1 font program.
    font: Type1RendererFont<'b>,
    /// Whether character codes should be treated as CIDs.
    is_cid: bool,
    /// Base glyph transform derived from the current text state.
    glyph_base_transform: Transform,
    /// Font design units per em, normalized during construction.
    units_per_em: u16,
}

/// A prepared glyph ready for drawing and text advance.
#[allow(clippy::large_enum_variant)]
enum PreparedGlyph<'a> {
    /// CFF/OpenType outline glyph data.
    Cff(CffPreparedGlyph<'a>),
    /// Classic Type 1 glyph data.
    Classic(ClassicPreparedGlyph),
}

struct CffPreparedGlyph<'a> {
    /// Resolved glyph identifier.
    gid: GlyphId,
    /// The resolved outline, if the font contains one.
    outline: Option<OutlineGlyph<'a>>,
}

struct ClassicPreparedGlyph {
    /// The path pen populated by the Type 1 renderer.
    pen: PdfPathPen,
    /// Preferred PDF width override, if present.
    pdf_width: Option<f32>,
    /// Width reported by the Type 1 draw operation, if available.
    glyph_width: Option<f32>,
}

/// Resolves a CID through the charset of a CID-keyed CFF font.
///
/// A CFF CID is not necessarily its glyph index. Missing CIDs use `.notdef`,
/// consistent with unresolved simple-font glyphs.
fn resolve_cid_cff_gid(cid: u16, charset: &CffCharset<'_>) -> GlyphId {
    charset.glyph_id(Sid::new(cid)).unwrap_or(GlyphId::NOTDEF)
}

impl<'a> PreparedGlyph<'a> {
    fn draw<C: CanvasBackend>(
        &mut self,
        canvas: &mut PdfCanvas<'_, C>,
        glyph_matrix: &Transform,
    ) -> Result<(), PdfCanvasError> {
        match self {
            Self::Cff(cff_glyph) => {
                if let Some(outline_glyph) = &cff_glyph.outline {
                    canvas.draw_outline_glyph(outline_glyph, glyph_matrix)?;
                }
            }
            Self::Classic(classic_glyph) => {
                classic_glyph.pen.path.transform(glyph_matrix);
                canvas.draw_glyph_path(&classic_glyph.pen.path)?;
            }
        }
        Ok(())
    }
}

impl<'a> Type1RendererFont<'a> {
    fn parse(
        font_bytes: &'a [u8],
        program_format: Type1FontProgramFormat,
        is_cid: bool,
    ) -> Result<ParsedType1Font<'a>, PdfCanvasError> {
        match program_format {
            Type1FontProgramFormat::OpenTypeCff => Self::parse_opentype_cff(font_bytes, is_cid),
            Type1FontProgramFormat::ClassicType1 => Self::parse_classic_type1(font_bytes),
        }
    }

    fn parse_opentype_cff(
        font_bytes: &'a [u8],
        is_cid: bool,
    ) -> Result<ParsedType1Font<'a>, PdfCanvasError> {
        let font_ref = FontRef::new(font_bytes).map_err(|_| Self::invalid_font_error())?;
        let units_per_em =
            normalized_units_per_em(font_ref.head().ok().map(|head| head.units_per_em()));
        let cid_charset = Self::load_cid_charset(&font_ref, is_cid)?;

        Ok(ParsedType1Font {
            font: Self::OpenTypeCff {
                outlines: font_ref.outline_glyphs(),
                font_ref,
                cid_charset,
            },
            units_per_em,
        })
    }

    fn parse_classic_type1(font_bytes: &'a [u8]) -> Result<ParsedType1Font<'a>, PdfCanvasError> {
        let font = ClassicType1Font::new(font_bytes).map_err(|_| Self::invalid_font_error())?;
        let units_per_em = normalized_units_per_em(u16::try_from(font.upem()).ok());

        Ok(ParsedType1Font {
            font: Self::ClassicType1(font),
            units_per_em,
        })
    }

    fn load_cid_charset(
        font_ref: &FontRef<'a>,
        is_cid: bool,
    ) -> Result<Option<CffCharset<'a>>, PdfCanvasError> {
        if !is_cid {
            return Ok(None);
        }

        let cff = font_ref.cff().map_err(|_| Self::cff_table_error())?;
        let cid_font = CffFontRef::new_cff(cff.offset_data().as_bytes(), 0, None)
            .map_err(|_| Self::invalid_font_error())?;

        cid_font
            .charset()
            .ok_or_else(Self::invalid_font_error)
            .map(Some)
    }

    fn invalid_font_error() -> PdfCanvasError {
        PdfCanvasError::InvalidFont("unrecognized Type 1 font data".into())
    }

    fn cff_table_error() -> PdfCanvasError {
        PdfCanvasError::InvalidFont("failed to read the CFF table from the Type 1 font".into())
    }

    fn cff_charset_error() -> PdfCanvasError {
        PdfCanvasError::InvalidFont("failed to read the Type 1 font CFF charset".into())
    }

    fn prepare_glyph<C: CanvasBackend>(
        &self,
        canvas: &PdfCanvas<'_, C>,
        is_cid: bool,
        char_code: u16,
    ) -> Result<PreparedGlyph<'a>, PdfCanvasError> {
        match self {
            Self::OpenTypeCff {
                font_ref,
                outlines,
                cid_charset,
            } => {
                let glyph_id = self.resolve_cff_gid(
                    canvas,
                    is_cid,
                    char_code,
                    font_ref,
                    cid_charset.as_ref(),
                )?;
                Ok(PreparedGlyph::Cff(CffPreparedGlyph {
                    gid: glyph_id,
                    outline: outlines.get(glyph_id),
                }))
            }
            Self::ClassicType1(classic_font) => {
                self.prepare_classic_glyph(canvas, is_cid, char_code, classic_font)
            }
        }
    }

    fn prepare_classic_glyph<C: CanvasBackend>(
        &self,
        canvas: &PdfCanvas<'_, C>,
        is_cid: bool,
        char_code: u16,
        classic_font: &ClassicType1Font,
    ) -> Result<PreparedGlyph<'a>, PdfCanvasError> {
        let glyph_id = self.resolve_classic_gid(canvas, is_cid, char_code, classic_font);
        let pdf_width = canvas
            .current_state()?
            .text_state
            .font
            .and_then(|font| font.glyph_width(char_code));

        let mut pen = PdfPathPen::default();
        let glyph_width = classic_font.draw(glyph_id, None, &mut pen).ok().flatten();

        Ok(PreparedGlyph::Classic(ClassicPreparedGlyph {
            pen,
            pdf_width,
            glyph_width,
        }))
    }

    fn advance_glyph<C: CanvasBackend>(
        &self,
        canvas: &mut PdfCanvas<'_, C>,
        char_code: u16,
        glyph: &PreparedGlyph<'a>,
        units_per_em: u16,
    ) -> Result<(), PdfCanvasError> {
        match (self, glyph) {
            (Self::OpenTypeCff { font_ref, .. }, PreparedGlyph::Cff(cff_glyph)) => canvas
                .current_state_mut()?
                .text_state
                .advance_horizontal_glyph(char_code, font_ref, cff_glyph.gid, units_per_em),
            (Self::ClassicType1(_), PreparedGlyph::Classic(classic_glyph)) => {
                classic_glyph.advance(canvas, char_code, units_per_em)
            }
            _ => Ok(()),
        }
    }

    fn resolve_cff_gid<C: CanvasBackend>(
        &self,
        canvas: &PdfCanvas<'_, C>,
        is_cid: bool,
        char_code: u16,
        font_ref: &FontRef<'_>,
        cid_charset: Option<&CffCharset<'_>>,
    ) -> Result<GlyphId, PdfCanvasError> {
        if is_cid {
            return Ok(cid_charset
                .map(|charset| resolve_cid_cff_gid(char_code, charset))
                .unwrap_or(GlyphId::NOTDEF));
        }

        let cff = font_ref.cff().map_err(|_| Self::cff_table_error())?;
        let charset = cff.charset(0).map_err(|_| Self::cff_charset_error())?;
        let glyph_name = canvas
            .current_state()?
            .text_state
            .glyph_name(char_code)
            .unwrap_or(".notdef");

        Ok(charset
            .and_then(|charset| {
                charset.iter().find_map(|(gid, cff_glyph_name)| {
                    cff_glyph_name
                        .resolve_standard()
                        .ok()
                        .filter(|standard_name| *standard_name == glyph_name.as_bytes())
                        .map(|_| gid)
                })
            })
            .unwrap_or(GlyphId::NOTDEF))
    }

    fn resolve_classic_gid<C: CanvasBackend>(
        &self,
        canvas: &PdfCanvas<'_, C>,
        is_cid: bool,
        char_code: u16,
        font: &ClassicType1Font,
    ) -> GlyphId {
        if is_cid {
            return GlyphId::new(u32::from(char_code));
        }

        let Ok(state) = canvas.current_state() else {
            return GlyphId::NOTDEF;
        };
        let glyph_name = state.text_state.glyph_name(char_code).unwrap_or(".notdef");

        font.glyph_names()
            .find_map(|(gid, current_name)| (current_name == glyph_name).then_some(gid))
            .unwrap_or(GlyphId::NOTDEF)
    }
}

impl ClassicPreparedGlyph {
    fn advance<C: CanvasBackend>(
        &self,
        canvas: &mut PdfCanvas<'_, C>,
        char_code: u16,
        units_per_em: u16,
    ) -> Result<(), PdfCanvasError> {
        let text_state = &mut canvas.current_state_mut()?.text_state;

        if let Some(pdf_width) = self.pdf_width {
            text_state.advance_horizontal_width(char_code, pdf_width, 1000);
            return Ok(());
        }

        if let Some(glyph_width) = self.glyph_width {
            text_state.advance_horizontal_width(char_code, glyph_width, units_per_em);
            return Ok(());
        }

        text_state.advance_horizontal_width(char_code, 0.0, units_per_em);
        Ok(())
    }
}

impl<'a, 'b, B: CanvasBackend> Type1FontRenderer<'a, 'b, B> {
    /// Constructs a Type 1 font renderer for the current canvas state.
    ///
    /// The constructor parses the font program, normalizes the font's
    /// `units_per_em`, and caches the glyph base transform needed during
    /// rendering.
    pub fn new(
        canvas: &'b mut PdfCanvas<'a, B>,
        font_bytes: &'b [u8],
        program_format: Type1FontProgramFormat,
        is_cid: bool,
    ) -> Result<Self, PdfCanvasError> {
        let ParsedType1Font { font, units_per_em } =
            Type1RendererFont::parse(font_bytes, program_format, is_cid)?;
        let glyph_base_transform = glyph_base_transform(&*canvas, units_per_em)?;

        Ok(Self {
            canvas,
            font,
            is_cid,
            glyph_base_transform,
            units_per_em,
        })
    }

    fn prepare_glyph(&self, char_code: u16) -> Result<PreparedGlyph<'b>, PdfCanvasError> {
        self.font
            .prepare_glyph(&*self.canvas, self.is_cid, char_code)
    }

    fn draw_glyph(
        &mut self,
        glyph: &mut PreparedGlyph<'b>,
        glyph_matrix: &Transform,
    ) -> Result<(), PdfCanvasError> {
        glyph.draw(self.canvas, glyph_matrix)
    }

    fn advance_text_position(
        &mut self,
        char_code: u16,
        glyph: &PreparedGlyph<'b>,
    ) -> Result<(), PdfCanvasError> {
        self.font
            .advance_glyph(self.canvas, char_code, glyph, self.units_per_em)
    }

    fn render_char(&mut self, char_code: u16) -> Result<(), PdfCanvasError> {
        let state = self.canvas.current_state()?;
        let text_state_before_advance = state.text_state.clone();
        let ctm = state.transform;
        let glyph_matrix = state
            .text_state
            .compose_glyph_matrix(self.glyph_base_transform, &state.transform);
        let mut glyph = self.prepare_glyph(char_code)?;
        self.draw_glyph(&mut glyph, &glyph_matrix)?;
        self.advance_text_position(char_code, &glyph)?;
        self.canvas
            .record_text_glyph(char_code, &text_state_before_advance, &ctm)
    }
}

impl<B: CanvasBackend> TextRenderer for Type1FontRenderer<'_, '_, B> {
    /// Renders text using the configured Type 1 font program.
    fn render_text(&mut self, iter: impl Iterator<Item = u16>) -> Result<(), PdfCanvasError> {
        for char_code in iter {
            self.render_char(char_code)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use read_fonts::FontData;

    use super::*;

    #[test]
    fn cid_keyed_cff_charset_maps_cids_to_glyph_ids() {
        // CFF format 0 charset: glyph 1 is CID 7 and glyph 2 is CID 42.
        const CID_CHARSET: [u8; 8] = [0xFF, 0xFF, 0xFF, 0, 0, 7, 0, 42];
        let charset = CffCharset::new(FontData::new(&CID_CHARSET), 3, 3).unwrap();

        assert_eq!(resolve_cid_cff_gid(7, &charset), GlyphId::new(1));
        assert_eq!(resolve_cid_cff_gid(42, &charset), GlyphId::new(2));
        assert_eq!(resolve_cid_cff_gid(99, &charset), GlyphId::NOTDEF);
    }
}
