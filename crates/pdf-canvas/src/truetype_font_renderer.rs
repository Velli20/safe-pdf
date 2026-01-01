use crate::{
    error::PdfCanvasError, pdf_canvas::PdfCanvas, pdf_path_pen::PdfPathPen,
    text_renderer::TextRenderer, text_state::TextState,
};
use pdf_graphics::{PaintMode, PathFillType, transform::Transform};
use read_fonts::TableProvider;
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlineGlyphCollection},
};
use thiserror::Error;

/// Fallback value for a font's `units_per_em` (design units per em).
///
/// In OpenType/TrueType fonts this normally comes from the `head` table and is
/// required to be in the range 16..=16384; a value of zero is invalid.
///
/// There is no universally correct fallback: many TrueType outlines use 2048
/// units/em, while Type 1 and OpenType/CFF outlines commonly use 1000.
///
/// We use 1000 here as a stable, PDF-friendly default (PDF text space is
/// conventionally scaled around 1000 units per em) that avoids division by zero
/// and keeps glyph scaling reasonable when the actual value is missing or
/// unusable.
const DEFAULT_UNITS_PER_EM: u16 = 1000;

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
pub(crate) struct TrueTypeFontRenderer<'a, 'b, T> {
    /// The canvas where glyphs are rendered.
    canvas: &'b mut PdfCanvas<'a, T>,
    /// Parsed TrueType font reference providing access to font tables.
    font_ref: FontRef<'a>,
    /// Collection of outline glyphs extracted from the font.
    outlines: OutlineGlyphCollection<'a>,
    /// Base transformation matrix for glyph rendering, incorporating font size,
    /// horizontal scaling, and text rise.
    glyph_base_transform: Transform,
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

impl<'a, 'b, T: std::error::Error> TrueTypeFontRenderer<'a, 'b, T> {
    pub fn new(
        canvas: &'b mut PdfCanvas<'a, T>,
        stream_object: &'a [u8],
        is_cid: bool,
    ) -> Result<Self, PdfCanvasError> {
        // Extract text state parameters for rendering.
        let TextState {
            horizontal_scaling,
            font_size,
            rise,
            ..
        } = canvas.current_state()?.text_state.clone();

        let font_ref = FontRef::new(stream_object)
            .map_err(|e| TrueTypeFontRendererError::FontParseError(e.to_string()))?;

        let outlines = font_ref.outline_glyphs();

        // Extract font and text state parameters.
        let units_per_em = font_ref
            .head()
            .ok()
            .map(|h| h.units_per_em())
            .filter(|&upe| upe != 0)
            .unwrap_or(DEFAULT_UNITS_PER_EM);

        // Compute the inverse of units per em for scaling.
        let upe_inv = 1.0 / f32::from(units_per_em);

        // Build the text rendering transform.
        let glyph_base_transform = Transform::from_row(
            font_size * upe_inv * horizontal_scaling, // sx
            0.0,                                      // ky (skew)
            0.0,                                      // kx (skew)
            font_size * upe_inv,                      // sy
            0.0,                                      // tx
            rise,                                     // ty
        );

        Ok(Self {
            font_ref,
            canvas,
            outlines,
            glyph_base_transform,
            is_cid,
        })
    }
}

impl<T: std::error::Error> TextRenderer for TrueTypeFontRenderer<'_, '_, T> {
    fn render_text(
        &mut self,
        text: &mut dyn Iterator<Item = u16>,
    ) -> Result<(), crate::error::PdfCanvasError> {
        // Extract text state parameters for rendering.
        let TextState {
            horizontal_scaling,
            font_size,
            character_spacing,
            word_spacing,
            ..
        } = self.canvas.current_state()?.text_state.clone();

        // Iterate over each character in the input text (1-byte encoding).
        for char_code in text {
            // Compose the final transformation matrix for this glyph:
            let mut glyph_matrix_for_char = self.glyph_base_transform;
            glyph_matrix_for_char.concat(&self.canvas.current_state()?.text_state.matrix);
            glyph_matrix_for_char.concat(&self.canvas.current_state()?.transform);

            // Build and fill the glyph outline.
            let glyph_id = if !self.is_cid {
                resolve_glyph_id(&self.font_ref, char_code)
            } else {
                GlyphId::new(u32::from(char_code))
            };

            if let Some(outline_glyph) = self.outlines.get(glyph_id) {
                let mut pen = PdfPathPen::default();
                let settings = DrawSettings::from((Size::unscaled(), LocationRef::default()));
                if outline_glyph.draw(settings, &mut pen).is_ok() {
                    pen.path.transform(&glyph_matrix_for_char);
                    self.canvas
                        .draw_path(&pen.path, PaintMode::Fill, PathFillType::Winding)?;
                }
            }

            let text_state = &mut self.canvas.current_state_mut()?.text_state;

            // Convert width from font units to ems and scale
            let w0_ems = text_state.glyph_width(char_code) / 1000.0;
            let glyph_width_tfs_scaled = w0_ems * font_size;

            // Apply word spacing only to space characters (0x20)
            let word_spacing_for_char = if char_code == 32 { word_spacing } else { 0.0 };

            // Compute and apply advance
            let advance_x = (glyph_width_tfs_scaled + character_spacing + word_spacing_for_char)
                * horizontal_scaling;
            text_state.matrix.post_translate(advance_x, 0.0);
        }
        Ok(())
    }
}
