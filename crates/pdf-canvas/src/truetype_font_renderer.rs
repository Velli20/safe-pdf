use crate::{
    canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
    text_renderer::TextRenderer, text_state::TextState,
};
use pdf_font::glyph_name_to_unicode::glyph_name_to_unicode;
use pdf_graphics::transform::Transform;
use read_fonts::TableProvider;
use skrifa::{
    FontRef, GlyphId, MetadataProvider, charmap::Charmap, outline::OutlineGlyphCollection,
};
use thiserror::Error;

/// Defines errors that can occur during TrueType font rendering.
#[derive(Debug, Error)]
pub enum TrueTypeFontRendererError {
    #[error("The font file object is not a stream, but a {found_type}")]
    FontFileNotStream { found_type: &'static str },
    #[error("Failed to parse the TrueType font file: {0}")]
    FontParseError(String),
    #[error("Incomplete 2-byte character at the end of the string")]
    IncompleteTwoByteCharacter,
}

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
}

/// Resolve a TrueType `GlyphId` for a given encoded character code.
///
/// Implements the glyph-mapping algorithm described in ISO 32000-1 §9.6.6.4
/// for non-CID TrueType fonts embedded in PDF.
///
/// # Resolution order
///
/// 1. **Encoding → AGL → cmap**: If a glyph name is available from the PDF
///    font's `/Encoding`, map it to a Unicode codepoint via the Adobe Glyph
///    List, then look up the codepoint in the font's best cmap subtable.
/// 2. **Direct Unicode**: Treat `char_code` as a Unicode scalar value and
///    probe the cmap (works for WinAnsiEncoding where codes ≈ Unicode).
/// 3. **Raw glyph index**: Use `char_code` as a raw glyph ID (last resort).
///
/// # Parameters
///
/// - `charmap`: Pre-computed skrifa `Charmap` (selects the best cmap subtable).
/// - `char_code`: The PDF text stream's 1-byte character code (widened to `u16`).
/// - `glyph_name`: Optional glyph name from the font's `/Encoding` dictionary.
fn resolve_glyph_id(charmap: &Charmap<'_>, char_code: u16, glyph_name: Option<&str>) -> GlyphId {
    // Step 1: Encoding -> glyph name -> Unicode (via AGL) -> cmap
    if let Some(name) = glyph_name
        && let Some(unicode_char) = glyph_name_to_unicode(name)
        && let Some(gid) = charmap.map(unicode_char)
    {
        return gid;
    }

    // Step 2: treat char_code as a Unicode codepoint directly
    if let Some(unicode_char) = char::from_u32(u32::from(char_code))
        && let Some(gid) = charmap.map(unicode_char)
    {
        return gid;
    }

    // Step 3: use the character code as a raw glyph index
    GlyphId::new(u32::from(char_code))
}

impl<'a, 'b, B: CanvasBackend> TrueTypeFontRenderer<'a, 'b, B> {
    pub fn new(
        canvas: &'b mut PdfCanvas<'a, B>,
        stream_object: &'a [u8],
        is_cid: bool,
    ) -> Result<Self, PdfCanvasError> {
        let font_ref = FontRef::new(stream_object)
            .map_err(|e| TrueTypeFontRendererError::FontParseError(e.to_string()))?;
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

            let glyph_id = if !self.is_cid {
                let glyph_name = state.text_state.glyph_name(char_code);
                resolve_glyph_id(&self.charmap, char_code, glyph_name)
            } else {
                GlyphId::new(u32::from(char_code))
            };

            if let Some(outline_glyph) = self.outlines.get(glyph_id) {
                self.canvas
                    .draw_outline_glyph(&outline_glyph, &glyph_matrix_for_char)?;
            }

            self.canvas
                .current_state_mut()?
                .text_state
                .advance_horizontal_glyph(char_code, &self.font_ref, glyph_id, self.units_per_em)?;
        }
        Ok(())
    }
}
