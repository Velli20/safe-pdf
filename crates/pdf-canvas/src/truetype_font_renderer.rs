use crate::{
    canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
    text_renderer::TextRenderer, text_state::TextState,
};
use pdf_graphics::transform::Transform;
use read_fonts::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider, outline::OutlineGlyphCollection};
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
/// This helper probes the font’s `cmap` tables to translate a 1- or 2-byte
/// character code (already decoded to `u16`) into the corresponding TrueType
/// `GlyphId`.
///
/// # Parameters:
///
/// - `font`: Parsed TrueType `FontRef` providing access to `cmap` tables.
/// - `char_code`: The PDF text stream’s character code.
///
/// # Returns:
///
/// The resolved `GlyphId` if a `cmap` entry is found, otherwise a fallback
/// `GlyphId(char_code)`.
fn resolve_glyph_id(font: &FontRef<'_>, char_code: u16) -> GlyphId {
    // Candidate character codes to probe in the cmap:
    // - The literal Unicode scalar value (for Unicode cmaps).
    // - 0xF000/0xF100 + code: common remappings used by symbol-encoded fonts
    //   (e.g., Windows “Symbol” encoding) where glyphs live in private-use ranges.
    let candidates = [
        u32::from(char_code),
        0xF000u32.saturating_add(u32::from(char_code)),
        0xF100u32.saturating_add(u32::from(char_code)),
    ];

    let mut resolved: Option<GlyphId> = None;

    'outer: for subtable in font.cmap().iter() {
        for code in candidates {
            if let Some(id) = subtable.map_codepoint(code) {
                resolved = Some(id);
                break 'outer;
            }
        }
    }

    if let Some(id) = resolved {
        return id;
    }

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
                resolve_glyph_id(&self.font_ref, char_code)
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
