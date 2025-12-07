use crate::{
    error::PdfCanvasError, pdf_canvas::PdfCanvas, text_renderer::TextRenderer,
    text_state::TextState,
};
use num_traits::FromPrimitive;
use pdf_graphics::{PaintMode, PathFillType, pdf_path::PdfPath, transform::Transform};
use thiserror::Error;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// Defines errors that can occur during TrueType font rendering.
#[derive(Debug, Error)]
pub enum TrueTypeFontRendererError {
    #[error("The font file object is not a stream, but a {found_type}")]
    FontFileNotStream { found_type: &'static str },
    #[error("Failed to parse the TrueType font file: {0:?}")]
    TtfParseError(ttf_parser::FaceParsingError),
    #[error("Incomplete 2-byte character at the end of the string")]
    IncompleteTwoByteCharacter,
    #[error("Not implemented")]
    NotImplemented,
}

/// A text renderer for TrueType-based fonts.
/// A unified wrapper that delegates to specialized renderers for TrueType-based fonts.
pub(crate) struct TrueTypeFontRenderer<'a, 'b, T> {
    canvas: &'b mut PdfCanvas<'a, T>,
    face: Face<'a>,
    glyph_base_transform: Transform,
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
/// - `face`: Parsed TrueType `Face` providing access to `cmap` tables.
/// - `char_code`: The PDF text stream’s character code.
///
/// # Returns:
///
/// The resolved `GlyphId` if a `cmap` entry is found, otherwise a fallback
/// `GlyphId(char_code)`.
fn glyph_id(face: &Face<'_>, char_code: u16) -> GlyphId {
    // Try to resolve the glyph using the TrueType cmap (character-to-glyph mapping).
    if let Some(cmap) = face.tables().cmap.as_ref() {
        // We'll search all cmap subtables and stop at the first match, if any.
        let mut resolved: Option<GlyphId> = None;

        // Candidate character codes to probe in the cmap:
        // - The literal Unicode scalar value (for Unicode cmaps).
        // - 0xF000/0xF100 + code: common remappings used by symbol-encoded fonts
        //   (e.g., Windows “Symbol” encoding) where glyphs live in private-use ranges.
        let candidates = [
            u32::from(char_code),
            0xF000u32.saturating_add(u32::from(char_code)),
            0xF100u32.saturating_add(u32::from(char_code)),
        ];

        'outer: for subtable in cmap.subtables {
            for code in candidates {
                if let Some(id) = subtable.glyph_index(code) {
                    resolved = Some(id);
                    break 'outer;
                }
            }
        }

        if let Some(id) = resolved {
            return id;
        }
    }

    GlyphId(char_code)
}

impl<'a, 'b, T: std::error::Error> TrueTypeFontRenderer<'a, 'b, T> {
    #[allow(clippy::too_many_arguments)]
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

        let face =
            Face::parse(stream_object, 0).map_err(TrueTypeFontRendererError::TtfParseError)?;

        // Extract font and text state parameters.
        let units_per_em = face.units_per_em();

        // Compute the inverse of units per em for scaling.
        let upe_inv = if units_per_em != 0 {
            1.0 / f32::from_u16(units_per_em)
                .ok_or(PdfCanvasError::NumericConversionError("units_per_em"))?
        } else {
            0.0
        };

        // Build the text rendering transform.
        let m_params = Transform::from_row(
            font_size * upe_inv * horizontal_scaling, // sx
            0.0,                                      // ky (skew)
            0.0,                                      // kx (skew)
            font_size * upe_inv,                      // sy
            0.0,                                      // tx
            rise,                                     // ty
        );

        Ok(Self {
            face,
            canvas,
            glyph_base_transform: m_params,
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
            let mut builder = PdfGlyphOutline::new(glyph_matrix_for_char);
            let glyph_id = if !self.is_cid {
                glyph_id(&self.face, char_code)
            } else {
                GlyphId(char_code)
            };
            self.face.outline_glyph(glyph_id, &mut builder);

            self.canvas
                .draw_path(&builder.path, PaintMode::Fill, PathFillType::Winding)?;

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

/// An implementation of `ttf_parser::OutlineBuilder` that converts glyph outlines
/// into a `PdfPath`.
#[derive(Default)]
pub struct PdfGlyphOutline {
    /// The `PdfPath` being constructed from the glyph outline commands.
    path: PdfPath,
    /// The transformation matrix to apply to each point of the glyph outline.
    transform: Transform,
}

impl PdfGlyphOutline {
    pub fn new(transform: Transform) -> Self {
        Self {
            path: PdfPath::default(),
            transform,
        }
    }
}

impl OutlineBuilder for PdfGlyphOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform.transform_point(x, y);
        self.path.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform.transform_point(x, y);
        self.path.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.transform.transform_point(x1, y1);
        let (x, y) = self.transform.transform_point(x, y);
        self.path.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.transform.transform_point(x1, y1);
        let (x2, y2) = self.transform.transform_point(x2, y2);
        let (x, y) = self.transform.transform_point(x, y);
        self.path.curve_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.path.close();
    }
}
