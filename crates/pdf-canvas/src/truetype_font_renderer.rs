use crate::{
    canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
    text_renderer::TextRenderer, text_state::TextState,
};
use pdf_font::font::Font;
use pdf_font::glyph_name_to_unicode::glyph_name_to_unicode;
use pdf_graphics::transform::Transform;
use read_fonts::TableProvider;
use skrifa::{
    FontRef, GlyphId, MetadataProvider, charmap::Charmap, outline::OutlineGlyphCollection,
};

/// Handles the conversion of TrueType glyph outlines into PDF path
/// operations, applying the appropriate transformations for font size,
/// scaling, and text positioning.
pub(crate) struct TrueTypeFontRenderer<'a, 'b, B: CanvasBackend> {
    /// The canvas where glyphs are rendered.
    canvas: &'b mut PdfCanvas<'a, B>,
    /// Parsed TrueType font reference providing access to font tables.
    font_ref: FontRef<'a>,
    /// Collection of outline glyphs extracted from the font.
    outlines: OutlineGlyphCollection<'a>,
    /// Pre-computed character-to-glyph mapping, selecting the best cmap
    /// subtable automatically (Unicode full repertoire > BMP > symbol).
    charmap: Charmap<'a>,
    /// Base transformation matrix for glyph rendering, incorporating font size,
    /// horizontal scaling, and text rise.
    glyph_base_transform: Transform,
    /// Font design units per em, read once from the `head` table in `new()`.
    units_per_em: u16,
    /// Whether this font uses CID (Character Identifier) encoding.
    is_cid: bool,
    /// Whether the font is flagged as symbolic (FontFlags::SYMBOLIC).
    is_symbolic: bool,
    /// Whether CID values should be mapped through Unicode before glyph lookup.
    map_cids_to_unicode: bool,
}

impl<'a, 'b, B: CanvasBackend> TrueTypeFontRenderer<'a, 'b, B> {
    fn resolve_simple_gid(
        &self,
        font: Option<&Font>,
        code: u16,
        glyph_name: Option<&str>,
    ) -> GlyphId {
        if let Some(unicode_char) = font.and_then(|current_font| current_font.char_to_unicode(code))
            && let Some(gid) = self.charmap.map(unicode_char)
        {
            return gid;
        }

        if let Some(name) = glyph_name
            && let Some(unicode_char) = glyph_name_to_unicode(name)
            && let Some(gid) = self.charmap.map(unicode_char)
        {
            return gid;
        }

        if let Ok(cmap) = self.font_ref.cmap()
            && let Some((_, _, subtable)) = cmap.best_subtable()
            && let Some(id) = subtable.map_codepoint(code)
        {
            return id;
        }

        if self.is_symbolic {
            if let Some(unicode_char) = char::from_u32(u32::from(code))
                && let Some(gid) = self.charmap.map(unicode_char)
            {
                return gid;
            }
        } else {
            println!("Warning: No glyph found for char code {}", code);
        }
        GlyphId::NOTDEF
    }

    pub fn new(
        canvas: &'b mut PdfCanvas<'a, B>,
        stream_object: &'a [u8],
        is_cid: bool,
        is_symbolic: bool,
        map_cids_to_unicode: bool,
    ) -> Result<Self, PdfCanvasError> {
        let font_ref = FontRef::new(stream_object)
            .map_err(|e| PdfCanvasError::TrueTypeFontParse(e.to_string()))?;
        let outlines = font_ref.outline_glyphs();
        let charmap = font_ref.charmap();

        let units_per_em = font_ref
            .head()
            .ok()
            .map(|h| h.units_per_em())
            .filter(|&upe| {
                (TextState::MIN_UNITS_PER_EM..=TextState::MAX_UNITS_PER_EM).contains(&upe)
            })
            .unwrap_or(TextState::DEFAULT_UNITS_PER_EM);

        let upe_inv = 1.0 / f32::from(units_per_em);

        let glyph_base_transform = canvas
            .current_state()?
            .text_state
            .glyph_base_transform(upe_inv);

        Ok(Self {
            font_ref,
            canvas,
            outlines,
            charmap,
            glyph_base_transform,
            units_per_em,
            is_cid,
            is_symbolic,
            map_cids_to_unicode,
        })
    }
}

impl<B: CanvasBackend> TextRenderer for TrueTypeFontRenderer<'_, '_, B> {
    fn render_text(
        &mut self,
        text: impl Iterator<Item = u16>,
    ) -> Result<(), crate::error::PdfCanvasError> {
        for char_code in text {
            let state = self.canvas.current_state()?;
            let glyph_matrix_for_char = state
                .text_state
                .compose_glyph_matrix(self.glyph_base_transform, &state.transform);

            // For CID fonts the char_code IS the glyph index by definition.
            // For non-CID fonts use the §9.6.6.4 resolver; None means the cmap
            // has no entry for this code — draw nothing but still advance.
            let resolved_glyph_id = if !self.is_cid {
                let current_font = state.text_state.font;
                let glyph_name = state.text_state.glyph_name(char_code);
                self.resolve_simple_gid(current_font, char_code, glyph_name)
            } else if self.map_cids_to_unicode {
                state
                    .text_state
                    .font
                    .and_then(|font| font.char_to_unicode(char_code))
                    .and_then(|unicode| self.charmap.map(unicode))
                    .unwrap_or(GlyphId::NOTDEF)
            } else {
                GlyphId::new(u32::from(char_code))
            };
            if resolved_glyph_id == GlyphId::NOTDEF {
                println!(
                    "Warning: No glyph found for char code {} (glyph name {:?})",
                    char_code,
                    state.text_state.glyph_name(char_code)
                );
            }
            if resolved_glyph_id != GlyphId::NOTDEF
                && let Some(outline_glyph) = self.outlines.get(resolved_glyph_id)
            {
                self.canvas
                    .draw_outline_glyph(&outline_glyph, &glyph_matrix_for_char)?;
            }
            self.canvas
                .current_state_mut()?
                .text_state
                .advance_horizontal_glyph(
                    char_code,
                    &self.font_ref,
                    resolved_glyph_id,
                    self.units_per_em,
                )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use pdf_font::{encoding::Encoding, flags::FontFlags, standard14::Standard14Font};

    use super::*;

    fn glyph_id_for(character: char) -> GlyphId {
        let fallback_bytes = Standard14Font::Helvetica.fallback_font_bytes();
        let font_ref =
            FontRef::new(fallback_bytes.as_ref()).expect("bundled fallback font must parse");
        font_ref
            .charmap()
            .map(character)
            .expect("fallback font must cover test character")
    }

    fn resolve_simple_gid_for_test(
        font: Option<&Font>,
        code: u16,
        glyph_name: Option<&str>,
        is_symbolic: bool,
    ) -> GlyphId {
        let fallback_bytes = Standard14Font::Helvetica.fallback_font_bytes();
        let font_ref =
            FontRef::new(fallback_bytes.as_ref()).expect("bundled fallback font must parse");
        let charmap = font_ref.charmap();

        if let Some(unicode_char) = font.and_then(|current_font| current_font.char_to_unicode(code))
            && let Some(gid) = charmap.map(unicode_char)
        {
            return gid;
        }

        if let Some(name) = glyph_name
            && let Some(unicode_char) = glyph_name_to_unicode(name)
            && let Some(gid) = charmap.map(unicode_char)
        {
            return gid;
        }

        if let Ok(cmap) = font_ref.cmap()
            && let Some((_, _, subtable)) = cmap.best_subtable()
            && let Some(id) = subtable.map_codepoint(code)
        {
            return id;
        }

        if is_symbolic
            && let Some(unicode_char) = char::from_u32(u32::from(code))
            && let Some(gid) = charmap.map(unicode_char)
        {
            return gid;
        }
        GlyphId::NOTDEF
    }

    #[test]
    fn simple_truetype_prefers_pdf_unicode_mapping() {
        let font = Font::TrueType(pdf_font::true_type_font::TrueTypeFont {
            font_file: Standard14Font::Helvetica.fallback_font_bytes(),
            widths: None,
            encoding: Some(Encoding::default()),
            to_unicode: None,
            standard14: Some(Standard14Font::Helvetica),
            flags: FontFlags::NON_SYMBOLIC,
        });

        let gid = resolve_simple_gid_for_test(Some(&font), 82, None, false);

        assert_eq!(gid, glyph_id_for('R'));
    }

    #[test]
    fn symbolic_simple_truetype_still_uses_pdf_unicode_mapping() {
        let font = Font::TrueType(pdf_font::true_type_font::TrueTypeFont {
            font_file: Standard14Font::Helvetica.fallback_font_bytes(),
            widths: None,
            encoding: Some(Encoding::default()),
            to_unicode: None,
            standard14: Some(Standard14Font::Helvetica),
            flags: FontFlags::SYMBOLIC | FontFlags::NON_SYMBOLIC,
        });

        let gid = resolve_simple_gid_for_test(Some(&font), 0x20, None, true);

        assert_eq!(gid, glyph_id_for(' '));
    }

    #[test]
    fn symbolic_simple_truetype_falls_back_to_unicode_codepoint_mapping() {
        let font = Font::TrueType(pdf_font::true_type_font::TrueTypeFont {
            font_file: Standard14Font::Helvetica.fallback_font_bytes(),
            widths: None,
            encoding: None,
            to_unicode: None,
            standard14: None,
            flags: FontFlags::SYMBOLIC,
        });

        let gid = resolve_simple_gid_for_test(Some(&font), 82, None, true);

        assert_eq!(gid, glyph_id_for('R'));
    }

    #[test]
    fn unmappable_simple_truetype_returns_notdef() {
        let font = Font::TrueType(pdf_font::true_type_font::TrueTypeFont {
            font_file: Cow::Owned(vec![]),
            widths: None,
            encoding: None,
            to_unicode: None,
            standard14: None,
            flags: FontFlags::SYMBOLIC,
        });

        let gid = resolve_simple_gid_for_test(Some(&font), 0x01, Some(".notdef"), true);

        assert_eq!(gid, GlyphId::NOTDEF);
    }
}
