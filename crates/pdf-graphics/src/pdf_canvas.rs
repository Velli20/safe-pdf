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
        }
    }
}

#[derive(Clone)] // Not Copy due to TextState<'a>
pub(crate) struct CanvasState<'a> {
    pub transform: Transform,
    pub stroke_color: Color,
    pub fill_color: Color,
    pub line_width: f32,
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
}

impl<'a> Default for CanvasState<'a> {
    fn default() -> Self {
        Self {
            transform: Transform::identity(),
            stroke_color: Self::DEFAULT_STROKE_COLOR,
            fill_color: Self::DEFAULT_FILL_COLOR,
            line_width: Self::DEFAULT_LINE_WIDTH,
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

    pub(crate) fn show_type3_font_text(&mut self, text: &[u8]) -> Result<(), PdfCanvasError> {
        let text_state = &self.current_state()?.text_state.clone();
        let current_font = text_state.font.ok_or(PdfCanvasError::NoCurrentFont)?;

        let type3_font = current_font.type3_font.as_ref().ok_or(PdfCanvasError::NoCurrentFont)?;

        let mut font_matrix = if let [a, b, c, d, e, f] = type3_font.font_matrix.as_slice() {
            Transform::from_row(
                *a, // Horizontal scaling (X axis).
                *b, // Horizontal skewing (Y axis).
                *c, // Vertical skewing (X axis).
                *d, // Vertical scaling (Y axis).
                *e, // Horizontal translation.
                *f, // Vertical translation.
            )
        } else {
            return Err(PdfCanvasError::InvalidFont("Invalid FontMatrix in Type3 font"));
        };

        // Per PDF spec 9.7.5, for Type 3 fonts, the glyph matrix is calculated as:
        // CTM_new = M_s * FontMatrix * Tm * CTM_old
        // where M_s = [Tfs*Th 0 0 Tfs 0 0]

        // Calculate M_s
        let th_factor = self.current_state()?.text_state.horizontal_scaling / 100.0;
        let text_font_size = self.current_state()?.text_state.font_size / 1000.0  ;

        font_matrix.scale(text_font_size, text_font_size);

        // Iterate over each character in the input text.
        let mut iter = text.iter();
        while let Some(char_code_byte) = iter.next() {
            // 1. For each character code in `text`:
            //    a. Use the font's encoding to map the character code to a character name.
            //    b. Look up the character name in the font's /CharProcs dictionary to get the
            //       glyph's content stream.
            //    c. Save the current graphics state.
            //    d. The glyph's content stream is executed. This requires parsing the stream's
            //       operators and calling the appropriate backend methods on the `canvas`. This
            //       is a recursive-like call to the content stream processor.
            //       - `let operators = PdfOperatorVariant::from(&glyph_stream_data)?;`
            //       - `for op in operators { op.call(canvas)?; }`
            //    e. The `d1` operator within the glyph stream sets the glyph's width. The
            //       backend needs to handle this and store the width.
            //    f. Restore the graphics state.
            //    g. Calculate the advance amount using the stored width, font size, character
            //       spacing, and word spacing.
            //    h. Update the text matrix `Tm` to position the next character.
            if let Some(encoding) = &type3_font.encoding {
                let glyph_name = encoding.differences.get(char_code_byte);
                if let Some(glyph_name) = glyph_name {
                    let char_procs = type3_font.char_procs.get(glyph_name);
                    if let Some(char_procs) = char_procs {
                        self.save()?;

                        // Concat with FontMatrix
                        // Concat with M_s
                        self.current_state_mut()?.transform.concat(&font_matrix);
                        self.current_state_mut()?.transform.concat(&text_state.matrix);
                        for op in char_procs {
                            op.call(self)?;
                        }
                        self.restore();
                        self.current_state_mut()?.text_state.line_matrix.translate(-4.0, 0.0);

                        // TODO: Advance the text matrix (Tm) using the glyph width.
                        // The width is set by the 'd1' operator within the char_procs stream.
                        // This backend needs to capture that width and use it here to update
                        // self.current_state_mut()?.text_state.matrix for the next character.
                    } else {
                        // If the glyph name is not present in the char proc map, then nothing shall be drawn.
                    }

                } else {
                    println!("No glyph_name");
                }
            } else {
                println!("No encoding");
            }
        }

        Ok(())
    }
}
