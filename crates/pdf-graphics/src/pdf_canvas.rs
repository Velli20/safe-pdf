use pdf_content_stream::graphics_state_operators::{LineCap, LineJoin};
use pdf_font::font::Font;
use pdf_page::page::PdfPage;
use ttf_parser::Face;

use crate::{
    CanvasBackend, PaintMode, PathFillType, color::Color, error::PdfCanvasError, pdf_path::PdfPath,
    transform::Transform,
};

/// Encapsulates text-specific state parameters.
#[derive(Clone)]
pub(crate) struct TextState<'a> {
    /// The text matrix (Tm), transforming text space to user space.
    pub(crate) matrix: Transform,
    /// The text line matrix (Tlm), tracking the start of the current line.
    pub(crate) line_matrix: Transform,
    /// Horizontal scaling of text (Th), as a percentage (default: 100.0).
    pub(crate) horizontal_scaling: f32,
    /// Font size (Tfs), in user space units.
    pub(crate) font_size: f32,
    /// Character spacing (Tc), in unscaled text space units.
    pub(crate) character_spacing: f32,
    /// Word spacing (Tw), in unscaled text space units.
    pub(crate) word_spacing: f32,
    /// Text rise (Ts), a vertical offset from the baseline, in unscaled text space units.
    pub(crate) rise: f32,
    /// The current font resource.
    pub(crate) font: Option<&'a Font>,
    /// The current font face object.
    pub(crate) font_face: Option<Face<'a>>,
    pub(crate) glyph_w: Option<f32>,
}

impl<'a> Default for TextState<'a> {
    fn default() -> Self {
        Self {
            matrix: Transform::identity(),
            line_matrix: Transform::identity(),
            horizontal_scaling: 100.0,
            font_size: 0.0,
            character_spacing: 0.0,
            word_spacing: 0.0,
            rise: 0.0,
            font: None,
            font_face: None,
            glyph_w: None,
        }
    }
}

#[derive(Clone)] // Not Copy due to TextState<'a>
pub(crate) struct CanvasState<'a> {
    pub transform: Transform,
    pub stroke_color: Color,
    pub fill_color: Color,
    pub line_width: f32,
    pub miter_limit: f32,
    pub text_state: TextState<'a>,
    pub clip_path: Option<PdfPath>,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
}

impl CanvasState<'_> {
    /// Default line width in user space units.
    const DEFAULT_LINE_WIDTH: f32 = 1.0;
    /// Default fill color.
    const DEFAULT_FILL_COLOR: Color = Color::from_rgb(0.0, 0.0, 0.0);
    /// Default stroke color.
    const DEFAULT_STROKE_COLOR: Color = Color::from_rgb(0.0, 0.0, 0.0);
    /// Default miter limit.
    const DEFAULT_MITER_LIMIT: f32 = 0.0;
}

impl<'a> Default for CanvasState<'a> {
    fn default() -> Self {
        Self {
            transform: Transform::identity(),
            stroke_color: Self::DEFAULT_STROKE_COLOR,
            fill_color: Self::DEFAULT_FILL_COLOR,
            line_width: Self::DEFAULT_LINE_WIDTH,
            miter_limit: Self::DEFAULT_MITER_LIMIT,
            text_state: TextState::default(),
            clip_path: None,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
        }
    }
}

pub struct PdfCanvas<'a> {
    pub(crate) current_path: Option<PdfPath>,
    pub(crate) canvas: &'a mut dyn CanvasBackend,
    pub(crate) page: &'a PdfPage,
    // Stores the graphics states, including text state.
    canvas_stack: Vec<CanvasState<'a>>,
}

impl<'a> PdfCanvas<'a> {
    pub fn new(backend: &'a mut dyn CanvasBackend, page: &'a PdfPage) -> Self {
        let media_box = &page.media_box;

        // Use descriptive names and ensure f32 type for PDF dimensions.
        // The `as f32` cast assumes media_box.width/height might return non-f32 types.
        let pdf_media_width = media_box.as_ref().unwrap().width() as f32;
        let pdf_media_height = media_box.as_ref().unwrap().height() as f32;

        // Backend dimensions are already f32 as per CanvasBackend trait.
        let backend_canvas_width = backend.width();
        let backend_canvas_height = backend.height();

        // Calculate scale factors.
        let scale_x = if pdf_media_width != 0.0 {
            backend_canvas_width / pdf_media_width
        } else {
            1.0
        };

        let scale_y = if pdf_media_height != 0.0 {
            backend_canvas_height / pdf_media_height
        } else {
            1.0
        };

        // Directly construct the userspace transformation matrix.
        // This matrix performs the following operations on PDF coordinates (px, py):
        // 1. Scales them: (px * scale_x, py * scale_y)
        // 2. Flips the Y-axis and translates it: Y_canvas = backend_canvas_height - (py * scale_y)
        // Resulting canvas coordinates: (px * scale_x, backend_canvas_height - py * scale_y)
        let userspace_matrix = Transform::from_row(
            scale_x,               // sx: Scale X
            0.0,                   // ky: Skew Y (none)
            0.0,                   // kx: Skew X (none)
            -scale_y,              // sy: Scale Y and reflect (Y points down)
            0.0,                   // tx: Translate X (none)
            backend_canvas_height, // ty: Translate Y to move origin to top-left after reflection
        );

        let canvas_stack = vec![CanvasState {
            transform: userspace_matrix,
            text_state: TextState::default(),
            ..Default::default()
        }];

        Self {
            current_path: None,
            canvas: backend,
            page,
            canvas_stack,
        }
    }

    /// Returns a reference to the current graphics state.
    pub(crate) fn current_state(&self) -> Result<&CanvasState<'a>, PdfCanvasError> {
        self.canvas_stack
            .last()
            .ok_or(PdfCanvasError::EmptyGraphicsStateStack)
    }

    /// Returns a mutable reference to the current graphics state.
    pub(crate) fn current_state_mut(&mut self) -> Result<&mut CanvasState<'a>, PdfCanvasError> {
        self.canvas_stack
            .last_mut()
            .ok_or(PdfCanvasError::EmptyGraphicsStateStack)
    }

    /// Saves a copy of the current graphics state onto the stack.
    pub(crate) fn save(&mut self) -> Result<(), PdfCanvasError> {
        let state = self.current_state()?.clone();
        self.canvas_stack.push(state);
        Ok(())
    }

    /// Restores the graphics state from the top of the stack.
    pub(crate) fn restore(&mut self) {
        let prev = self.canvas_stack.pop();
        if let Some(state) = prev {
            if state.clip_path.is_some() {
                self.canvas.reset_clip();
            }
        }
    }

    pub(crate) fn paint_taken_path(
        &mut self,
        mode: PaintMode,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        if let Some(mut path) = self.current_path.take() {
            path.transform(&self.current_state()?.transform)?;
            if mode == PaintMode::Fill {
                self.canvas
                    .fill_path(&path, fill_type, self.current_state()?.fill_color);
            } else {
                self.canvas.stroke_path(
                    &path,
                    self.current_state()?.stroke_color,
                    self.current_state()?.line_width,
                );
            }
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }

    /// Renders a text string using a Type 3 font.
    ///
    /// This method processes a sequence of character codes, looks up the corresponding
    /// glyph procedures from the Type 3 font's `CharProcs` dictionary, and executes
    /// them to draw the glyphs. It handles the specific transformation logic required
    /// for Type 3 fonts, including the font matrix, font size, text matrix, and CTM.
    pub(crate) fn show_type3_font_text(&mut self, text: &[u8]) -> Result<(), PdfCanvasError> {
        let text_state = &self.current_state()?.text_state.clone();
        let current_font = text_state.font.ok_or(PdfCanvasError::NoCurrentFont)?;

        let type3_font = current_font
            .type3_font
            .as_ref()
            .ok_or(PdfCanvasError::NoCurrentFont)?;

        let font_matrix = if let [a, b, c, d, e, f] = type3_font.font_matrix.as_slice() {
            Transform::from_row(*a, *b, *c, *d, *e, *f)
        } else {
            return Err(PdfCanvasError::InvalidFont(
                "Invalid FontMatrix in Type3 font",
            ));
        };

        // For Type 3 fonts, the final transformation for a glyph is CTM * Tm * S * FontMatrix,
        // where S is a matrix for font size (Tfs), horizontal scaling (Th), and rise (Ts).
        // S = [Tfs * Th 0 0 Tfs 0 Ts].
        // We pre-calculate this combined matrix. `concat` performs pre-multiplication (other * self).
        let th_factor = text_state.horizontal_scaling / 100.0;
        let font_size_matrix = Transform::from_row(
            text_state.font_size * th_factor, // sx
            0.0,                              // ky
            0.0,                              // kx
            text_state.font_size,             // sy
            0.0,                              // tx
            text_state.rise,                  // ty
        );

        // For each character code, we will render its glyph.
        let mut iter = text.iter();
        while let Some(char_code_byte) = iter.next() {
            let mut text_rendering_matrix = font_matrix.clone();
            text_rendering_matrix.concat(&font_size_matrix); // S * FontMatrix
            text_rendering_matrix.concat(&self.current_state()?.text_state.matrix); // Tm * (S * FontMatrix)
            text_rendering_matrix.concat(&self.current_state()?.transform); // CTM * (Tm * S * FontMatrix)

            // Step 1: Map character code to glyph name using the font's encoding.
            let glyph_name = type3_font
                .encoding
                .as_ref()
                .and_then(|enc| enc.differences.get(char_code_byte));

            if let Some(glyph_name) = glyph_name {
                // Step 2: Look up the glyph's content stream from the CharProcs dictionary.
                if let Some(char_procs) = type3_font.char_procs.get(glyph_name) {
                    // Step 3: Save graphics state before drawing the glyph.
                    self.save()?;

                    // Step 4: Set the transformation matrix for the glyph and execute its content stream.
                    // The CTM is temporarily replaced with the computed text rendering matrix.
                    self.current_state_mut()?.transform = text_rendering_matrix.clone();
                    for op in char_procs {
                        op.call(self)?;
                    }

                    let gw = self.current_state_mut()?.text_state.glyph_w;
                    // Step 5: Restore the original graphics state.
                    self.restore();

                    // TODO: Step 6: Advance the text matrix (Tm) using the glyph width.
                    // The width is set by the 'd1' operator within the char_procs stream.
                    // This backend needs to capture that width and use it here to update
                    // self.current_state_mut()?.text_state.matrix for the next character.
                    if let Some(width) = gw {
                        let advance = width * text_state.font_size / 1000.0;
                        self.current_state_mut()?
                            .text_state
                            .matrix
                            .translate(advance, 0.0);
                    }
                }
                // If the glyph name is not in CharProcs, nothing is drawn.
            }
        }

        Ok(())
    }
}
