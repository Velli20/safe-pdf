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
use skrifa::outline::OutlineGlyphCollection;
use skrifa::{FontRef, GlyphId, MetadataProvider};

enum Type1RendererFont<'a> {
    OpenTypeCff {
        font_ref: FontRef<'a>,
        outlines: OutlineGlyphCollection<'a>,
        cid_charset: Option<CffCharset<'a>>,
    },
    ClassicType1(ClassicType1Font),
}

/// Renders OpenType/CFF and classic Type 1 fonts.
pub(crate) struct Type1FontRenderer<'a, 'b, B: CanvasBackend> {
    canvas: &'b mut PdfCanvas<'a, B>,
    font: Type1RendererFont<'b>,
    is_cid: bool,
    glyph_base_transform: Transform,
    units_per_em: u16,
}

/// Resolves a CID through the charset of a CID-keyed CFF font.
///
/// A CFF CID is not necessarily its glyph index. Missing CIDs use `.notdef`,
/// consistent with unresolved simple-font glyphs.
fn resolve_cid_cff_gid(cid: u16, charset: &CffCharset<'_>) -> GlyphId {
    charset.glyph_id(Sid::new(cid)).unwrap_or(GlyphId::NOTDEF)
}

fn cff_gid(
    font_ref: &FontRef<'_>,
    cid_charset: Option<&CffCharset<'_>>,
    is_cid: bool,
    char_code: u16,
    glyph_name: Option<&str>,
) -> Result<GlyphId, PdfCanvasError> {
    if is_cid {
        return Ok(cid_charset
            .map(|charset| resolve_cid_cff_gid(char_code, charset))
            .unwrap_or(GlyphId::NOTDEF));
    }

    let charset = font_ref
        .cff()
        .map_err(|_| PdfCanvasError::InvalidType1CffTable)?
        .charset(0)
        .map_err(|_| PdfCanvasError::InvalidType1CffCharset)?;
    let glyph_name = glyph_name.unwrap_or(".notdef").as_bytes();

    Ok(charset
        .and_then(|charset| {
            charset.iter().find_map(|(gid, name)| {
                name.resolve_standard()
                    .ok()
                    .filter(|name| *name == glyph_name)
                    .map(|_| gid)
            })
        })
        .unwrap_or(GlyphId::NOTDEF))
}

fn classic_gid(
    font: &ClassicType1Font,
    is_cid: bool,
    char_code: u16,
    glyph_name: Option<&str>,
) -> GlyphId {
    if is_cid {
        return GlyphId::new(u32::from(char_code));
    }

    let glyph_name = glyph_name.unwrap_or(".notdef");
    font.glyph_names()
        .find_map(|(gid, name)| (name == glyph_name).then_some(gid))
        .unwrap_or(GlyphId::NOTDEF)
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
        match program_format {
            Type1FontProgramFormat::OpenTypeCff => {
                let font_ref =
                    FontRef::new(font_bytes).map_err(|_| PdfCanvasError::UnrecognizedType1Font)?;
                let units_per_em =
                    normalized_units_per_em(font_ref.head().ok().map(|head| head.units_per_em()));
                let cid_charset = if is_cid {
                    let cff = font_ref
                        .cff()
                        .map_err(|_| PdfCanvasError::InvalidType1CffTable)?;
                    let cid_font = CffFontRef::new_cff(cff.offset_data().as_bytes(), 0, None)
                        .map_err(|_| PdfCanvasError::UnrecognizedType1Font)?;
                    Some(
                        cid_font
                            .charset()
                            .ok_or(PdfCanvasError::UnrecognizedType1Font)?,
                    )
                } else {
                    None
                };
                let glyph_base_transform = glyph_base_transform(&*canvas, units_per_em)?;

                Ok(Self {
                    canvas,
                    font: Type1RendererFont::OpenTypeCff {
                        outlines: font_ref.outline_glyphs(),
                        font_ref,
                        cid_charset,
                    },
                    is_cid,
                    glyph_base_transform,
                    units_per_em,
                })
            }
            Type1FontProgramFormat::ClassicType1 => {
                let font = ClassicType1Font::new(font_bytes)
                    .map_err(|_| PdfCanvasError::UnrecognizedType1Font)?;
                let units_per_em = normalized_units_per_em(u16::try_from(font.upem()).ok());
                let glyph_base_transform = glyph_base_transform(&*canvas, units_per_em)?;

                Ok(Self {
                    canvas,
                    font: Type1RendererFont::ClassicType1(font),
                    is_cid,
                    glyph_base_transform,
                    units_per_em,
                })
            }
        }
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

        for char_code in iter {
            let text_glyph_start = canvas.text_glyph_start()?;
            let state = canvas.current_state()?;
            let glyph_matrix = state
                .text_state
                .compose_glyph_matrix(*glyph_base_transform, &state.transform);
            let glyph_name = state.text_state.glyph_name(char_code);

            match &*font {
                Type1RendererFont::OpenTypeCff {
                    font_ref,
                    outlines,
                    cid_charset,
                } => {
                    let gid = cff_gid(
                        font_ref,
                        cid_charset.as_ref(),
                        *is_cid,
                        char_code,
                        glyph_name,
                    )?;
                    if let Some(outline) = outlines.get(gid) {
                        canvas.draw_outline_glyph(&outline, &glyph_matrix)?;
                    }
                    canvas
                        .current_state_mut()?
                        .text_state
                        .advance_horizontal_glyph(char_code, font_ref, gid, *units_per_em)?;
                }
                Type1RendererFont::ClassicType1(classic_font) => {
                    let gid = classic_gid(classic_font, *is_cid, char_code, glyph_name);
                    let pdf_width = state
                        .text_state
                        .font
                        .and_then(|font| font.glyph_width(char_code));
                    let mut pen = PdfPathPen::default();
                    let glyph_width = classic_font.draw(gid, None, &mut pen).ok().flatten();

                    pen.path.transform(&glyph_matrix);
                    canvas.draw_glyph_path(&pen.path)?;

                    let text_state = &mut canvas.current_state_mut()?.text_state;
                    match (pdf_width, glyph_width) {
                        (Some(width), _) => {
                            text_state.advance_horizontal_width(char_code, width, 1000);
                        }
                        (_, Some(width)) => {
                            text_state.advance_horizontal_width(char_code, width, *units_per_em);
                        }
                        _ => text_state.advance_horizontal_width(char_code, 0.0, *units_per_em),
                    }
                }
            }

            canvas.record_text_glyph(char_code, text_glyph_start)?;
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
