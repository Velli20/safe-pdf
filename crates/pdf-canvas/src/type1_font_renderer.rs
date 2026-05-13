use crate::canvas_backend::CanvasBackend;
use crate::pdf_canvas::PdfCanvas;
use crate::pdf_path_pen::PdfPathPen;
use crate::text_state::TextState;
use crate::{error::PdfCanvasError, text_renderer::TextRenderer};
use pdf_font::type1_font::Type1FontProgramFormat;
use pdf_graphics::transform::Transform;
use read_fonts::TableProvider;
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
    },
    /// A classic Type 1 font program.
    ClassicType1(ClassicType1Font),
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
    Cff {
        /// Resolved glyph identifier.
        gid: GlyphId,
        /// The resolved outline, if the font contains one.
        outline: Option<OutlineGlyph<'a>>,
    },
    /// Classic Type 1 glyph data.
    Classic {
        /// The path pen populated by the Type 1 renderer.
        pen: PdfPathPen,
        /// Preferred PDF width override, if present.
        pdf_width: Option<f32>,
        /// Width reported by the Type 1 draw operation, if available.
        glyph_width: Option<f32>,
    },
}

/// Returns the canonical invalid-font error used by this renderer.
fn invalid_type1_font_error() -> PdfCanvasError {
    PdfCanvasError::InvalidFont("unrecognized Type 1 font data".into())
}

/// Normalizes a parsed `units_per_em` value into the range accepted by the text state.
fn normalized_units_per_em(units_per_em: Option<u16>) -> u16 {
    units_per_em
        .filter(|&upe| (TextState::MIN_UNITS_PER_EM..=TextState::MAX_UNITS_PER_EM).contains(&upe))
        .unwrap_or(TextState::DEFAULT_UNITS_PER_EM)
}

/// Resolves the glyph identifier for a CFF/OpenType Type 1 font.
fn resolve_cff_gid<'a, B: CanvasBackend>(
    canvas: &PdfCanvas<'a, B>,
    is_cid: bool,
    char_code: u16,
    font_ref: &FontRef<'_>,
) -> Result<GlyphId, PdfCanvasError> {
    if is_cid {
        return Ok(GlyphId::new(u32::from(char_code)));
    }

    let cff = font_ref.cff().map_err(|_| {
        PdfCanvasError::InvalidFont("failed to read the CFF table from the Type 1 font".into())
    })?;

    let charset = cff.charset(0).map_err(|_| {
        PdfCanvasError::InvalidFont("failed to read the Type 1 font CFF charset".into())
    })?;

    let state = canvas.current_state()?;
    let name = state.text_state.glyph_name(char_code).unwrap_or(".notdef");

    Ok(charset
        .and_then(|charset| {
            charset.iter().find_map(|(gid, glyph_name)| {
                let is_match = glyph_name
                    .resolve_standard()
                    .map(|standard_name| standard_name == name.as_bytes())
                    .unwrap_or(false);
                if is_match { Some(gid) } else { None }
            })
        })
        .unwrap_or(GlyphId::NOTDEF))
}

/// Resolves the glyph identifier for a classic Type 1 font.
fn resolve_classic_gid<'a, B: CanvasBackend>(
    canvas: &PdfCanvas<'a, B>,
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
    let name = state.text_state.glyph_name(char_code).unwrap_or(".notdef");

    font.glyph_names()
        .find_map(|(gid, glyph_name)| (glyph_name == name).then_some(gid))
        .unwrap_or(GlyphId::NOTDEF)
}

/// Prepares the glyph data needed to render one character code.
fn prepare_type1_glyph<'font, C: CanvasBackend>(
    font: &Type1RendererFont<'font>,
    canvas: &PdfCanvas<'_, C>,
    is_cid: bool,
    char_code: u16,
) -> Result<PreparedGlyph<'font>, PdfCanvasError> {
    match font {
        Type1RendererFont::OpenTypeCff { font_ref, outlines } => {
            let gid = resolve_cff_gid(canvas, is_cid, char_code, font_ref)?;
            Ok(PreparedGlyph::Cff {
                gid,
                outline: outlines.get(gid),
            })
        }
        Type1RendererFont::ClassicType1(classic_font) => {
            let gid = resolve_classic_gid(canvas, is_cid, char_code, classic_font);
            let state = canvas.current_state()?;
            let pdf_width = state
                .text_state
                .font
                .and_then(|font| font.glyph_width(char_code));

            let mut pen = PdfPathPen::default();
            let glyph_width = classic_font.draw(gid, None, &mut pen).ok().flatten();

            Ok(PreparedGlyph::Classic {
                pen,
                pdf_width,
                glyph_width,
            })
        }
    }
}

/// Draws a prepared glyph using the supplied glyph matrix.
fn draw_prepared_type1_glyph<'font, C: CanvasBackend>(
    font: &Type1RendererFont<'font>,
    canvas: &mut PdfCanvas<'_, C>,
    glyph_matrix: &Transform,
    glyph: &mut PreparedGlyph<'font>,
) -> Result<(), PdfCanvasError> {
    match (font, glyph) {
        (Type1RendererFont::OpenTypeCff { .. }, PreparedGlyph::Cff { outline, .. }) => {
            if let Some(outline_glyph) = outline {
                canvas.draw_outline_glyph(outline_glyph, glyph_matrix)?;
            }
        }
        (Type1RendererFont::ClassicType1(_), PreparedGlyph::Classic { pen, .. }) => {
            pen.path.transform(glyph_matrix);
            canvas.draw_glyph_path(&pen.path)?;
        }
        _ => unreachable!("prepared glyph and font format must match"),
    }
    Ok(())
}

/// Advances the text cursor after a prepared glyph has been drawn.
fn advance_prepared_type1_glyph<'font, C: CanvasBackend>(
    font: &Type1RendererFont<'font>,
    canvas: &mut PdfCanvas<'_, C>,
    char_code: u16,
    glyph: &PreparedGlyph<'font>,
    units_per_em: u16,
) -> Result<(), PdfCanvasError> {
    match (font, glyph) {
        (Type1RendererFont::OpenTypeCff { font_ref, .. }, PreparedGlyph::Cff { gid, .. }) => canvas
            .current_state_mut()?
            .text_state
            .advance_horizontal_glyph(char_code, font_ref, *gid, units_per_em),
        (
            Type1RendererFont::ClassicType1(_),
            PreparedGlyph::Classic {
                pdf_width,
                glyph_width,
                ..
            },
        ) => {
            let text_state = &mut canvas.current_state_mut()?.text_state;
            if let Some(pdf_width) = pdf_width {
                text_state.advance_horizontal_width(
                    char_code,
                    *pdf_width,
                    TextState::DEFAULT_UNITS_PER_EM,
                );
            } else if let Some(glyph_width) = glyph_width {
                text_state.advance_horizontal_width(char_code, *glyph_width, units_per_em);
            } else {
                text_state.advance_horizontal_width(char_code, 0.0, units_per_em);
            }
            Ok(())
        }
        _ => unreachable!("prepared glyph and font format must match"),
    }
}

/// Renders text for a Type 1 font using a single shared glyph loop.
fn render_type1_text<'font, C: CanvasBackend>(
    font: &Type1RendererFont<'font>,
    canvas: &mut PdfCanvas<'_, C>,
    is_cid: bool,
    glyph_base_transform: Transform,
    units_per_em: u16,
    iter: impl Iterator<Item = u16>,
) -> Result<(), PdfCanvasError> {
    for char_code in iter {
        let glyph_matrix = {
            let state = canvas.current_state()?;
            state
                .text_state
                .compose_glyph_matrix(glyph_base_transform, &state.transform)
        };

        let mut glyph = prepare_type1_glyph(font, &*canvas, is_cid, char_code)?;

        draw_prepared_type1_glyph(font, canvas, &glyph_matrix, &mut glyph)?;
        advance_prepared_type1_glyph(font, canvas, char_code, &glyph, units_per_em)?;
    }
    Ok(())
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
        let (font, units_per_em) = match program_format {
            Type1FontProgramFormat::OpenTypeCff => {
                let font_ref = FontRef::new(font_bytes).map_err(|_| invalid_type1_font_error())?;
                let units_per_em =
                    normalized_units_per_em(font_ref.head().ok().map(|head| head.units_per_em()));

                (
                    Type1RendererFont::OpenTypeCff {
                        outlines: font_ref.outline_glyphs(),
                        font_ref,
                    },
                    units_per_em,
                )
            }
            Type1FontProgramFormat::ClassicType1 => {
                let font =
                    ClassicType1Font::new(font_bytes).map_err(|_| invalid_type1_font_error())?;
                let units_per_em = normalized_units_per_em(u16::try_from(font.upem()).ok());
                (Type1RendererFont::ClassicType1(font), units_per_em)
            }
        };

        let upe_inv = 1.0 / f32::from(units_per_em);
        let glyph_base_transform = canvas
            .current_state()?
            .text_state
            .glyph_base_transform(upe_inv);

        Ok(Self {
            canvas,
            font,
            is_cid,
            glyph_base_transform,
            units_per_em,
        })
    }
}

impl<B: CanvasBackend> TextRenderer for Type1FontRenderer<'_, '_, B> {
    /// Renders text using the configured Type 1 font program.
    fn render_text(&mut self, iter: impl Iterator<Item = u16>) -> Result<(), PdfCanvasError> {
        let Self {
            canvas,
            font,
            is_cid,
            glyph_base_transform,
            units_per_em,
        } = self;

        render_type1_text(
            font,
            canvas,
            *is_cid,
            *glyph_base_transform,
            *units_per_em,
            iter,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use pdf_font::{
        encoding::Encoding,
        font::Font,
        type1_font::{Type1Font, Type1FontProgramFormat},
    };
    use pdf_graphics::{BlendMode, MaskMode, PathFillType, color::Color, rect::Rect};
    use pdf_page::page::PdfPage;

    use crate::{
        canvas_backend::{CanvasBackend, Image, Shader},
        recording_canvas::RecordingCanvas,
    };

    use super::*;

    const EEXEC_SEED: u16 = 55665;

    #[derive(Default)]
    struct FillCountingCanvas {
        fill_count: usize,
    }

    impl CanvasBackend for FillCountingCanvas {
        fn fill_path(
            &mut self,
            _path: &pdf_graphics::pdf_path::PdfPath,
            _fill_type: PathFillType,
            _color: Color,
            _shader: &Option<Shader>,
            _blend_mode: Option<BlendMode>,
        ) -> Result<(), PdfCanvasError> {
            self.fill_count += 1;
            Ok(())
        }

        fn stroke_path(
            &mut self,
            _path: &pdf_graphics::pdf_path::PdfPath,
            _color: Color,
            _line_width: f32,
            _shader: &Option<Shader>,
            _blend_mode: Option<BlendMode>,
        ) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn set_clip_region(
            &mut self,
            _path: &pdf_graphics::pdf_path::PdfPath,
            _mode: PathFillType,
        ) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn width(&self) -> f32 {
            100.0
        }

        fn height(&self) -> f32 {
            100.0
        }

        fn save(&mut self) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn restore(&mut self) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn draw_image_rect(
            &mut self,
            _image: &Image<'_>,
            _blend_mode: Option<BlendMode>,
            _dest_rect: Rect,
            _image_rotation: Option<f32>,
        ) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn begin_mask_layer(
            &mut self,
            _mask: &Arc<RecordingCanvas>,
            _transform: &Transform,
            _mask_mode: MaskMode,
        ) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn end_mask_layer(
            &mut self,
            _mask: &Arc<RecordingCanvas>,
            _transform: &Transform,
            _mask_mode: MaskMode,
        ) -> Result<(), PdfCanvasError> {
            Ok(())
        }
    }

    fn encrypt(bytes: &[u8], seed: u16) -> Vec<u8> {
        let mut r = seed;
        let mut out = Vec::with_capacity(bytes.len());
        for &plain in bytes {
            let cipher = plain ^ ((r >> 8) as u8);
            out.push(cipher);
            r = u16::try_from(
                (u32::from(cipher) + u32::from(r))
                    .wrapping_mul(52845)
                    .wrapping_add(22719)
                    & 0xFFFF,
            )
            .unwrap();
        }
        out
    }

    fn minimal_classic_type1_font() -> Vec<u8> {
        let cleartext = br#"%!FontType1-1.0: DummyFont 1.0
10 dict begin
/FontName /DummyFont def
/FontType 1 def
/FontMatrix [0.001 0 0 0.001 0 0] readonly def
/FontBBox [0 0 0 0] readonly def
/Encoding StandardEncoding def
currentdict end
currentfile eexec
"#;
        let private_plain = b"/Private 1 dict dup begin\n/lenIV -1 def\n/CharStrings 1 dict dup begin\n/.notdef 1 RD \x0E ND\nend\nend\nmark currentfile closefile\n";
        let mut encrypted_private = vec![0, 0, 0, 0];
        encrypted_private.extend_from_slice(private_plain);
        let encrypted_private = encrypt(&encrypted_private, EEXEC_SEED);

        let mut bytes = cleartext.to_vec();
        bytes.extend_from_slice(&encrypted_private);
        bytes.extend_from_slice(b"0000000000000000000000000000000000000000\ncleartomark\n");
        bytes
    }

    fn page() -> PdfPage {
        PdfPage {
            contents: None,
            media_box: None,
            resources: None,
        }
    }

    #[test]
    fn classic_type1_renderer_uses_pdf_widths_for_advance() {
        let font = Font::Type1(Type1Font {
            font_file: minimal_classic_type1_font(),
            program_format: Type1FontProgramFormat::ClassicType1,
            widths: Some(HashMap::from([(65, 500.0)])),
            encoding: Encoding::default(),
            to_unicode: None,
        });

        let page = page();
        let mut backend = FillCountingCanvas::default();
        let mut canvas = PdfCanvas::new(&mut backend, &page, None).unwrap();
        {
            let state = canvas.current_state_mut().unwrap();
            state.text_state.font = Some(&font);
            state.text_state.font_size = 10.0;
        }

        let mut renderer = Type1FontRenderer::new(
            &mut canvas,
            match &font {
                Font::Type1(font) => font.font_file.as_slice(),
                _ => unreachable!(),
            },
            Type1FontProgramFormat::ClassicType1,
            false,
        )
        .unwrap();

        renderer.render_text([65].into_iter()).unwrap();

        assert_eq!(canvas.canvas.fill_count, 1);
        assert_eq!(canvas.current_state().unwrap().text_state.matrix.tx, 5.0);
    }
}
