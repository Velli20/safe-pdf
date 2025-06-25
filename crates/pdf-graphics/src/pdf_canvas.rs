use pdf_content_stream::graphics_state_operators::{LineCap, LineJoin};
use pdf_font::font::Font;
use pdf_page::page::PdfPage;

use crate::{
    PaintMode, PathFillType, canvas::Canvas, canvas_backend::CanvasBackend, color::Color,
    error::PdfCanvasError, pdf_path::PdfPath, transform::Transform,
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
        }
    }
}

#[derive(Clone)]
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

impl Canvas for PdfCanvas<'_> {
    fn save(&mut self) -> Result<(), PdfCanvasError> {
        let state = self.current_state()?.clone();
        self.canvas_stack.push(state);
        Ok(())
    }

    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        let prev = self.canvas_stack.pop();
        if let Some(state) = prev {
            if state.clip_path.is_some() {
                self.canvas.reset_clip();
            }
        }
        Ok(())
    }

    fn set_matrix(&mut self, matrix: Transform) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.transform = matrix;
        Ok(())
    }

    fn translate(&mut self, tx: f32, ty: f32) -> Result<(), PdfCanvasError> {
        todo!()
    }

    fn fill_path(&mut self, path: &PdfPath, fill_type: PathFillType) -> Result<(), PdfCanvasError> {
        let fill_color = self.current_state()?.fill_color;
        self.canvas.fill_path(path, fill_type, fill_color);
        Ok(())
    }
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

    pub(crate) fn paint_taken_path(
        &mut self,
        mode: PaintMode,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        if let Some(mut path) = self.current_path.take() {
            path.transform(&self.current_state()?.transform);
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
}
