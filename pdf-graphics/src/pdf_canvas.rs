use pdf_font::font::Font;
use pdf_page::page::PdfPage;
use ttf_parser::Face;

use crate::{
    CanvasBackend, PaintMode, PathFillType, color::Color, error::PdfCanvasError, pdf_path::PdfPath,
    transform::Transform,
};

/// Encapsulates text-specific state parameters.
/// These parameters are part of the PDF graphics state and can be saved and restored.
#[derive(Clone)] // Not Copy due to Face<'a>
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
    /// The parsed ttf_parser Face for the current font.
    pub(crate) font_face: Option<Face<'a>>,
}

impl<'a> Default for TextState<'a> {
    fn default() -> Self {
        Self {
            matrix: Transform::identity(),      // Initialized by BT operator
            line_matrix: Transform::identity(), // Initialized by BT operator
            horizontal_scaling: 100.0,          // PDF spec default
            font_size: 0.0, // PDF spec: undefined, must be set by Tf. Using 0 as placeholder.
            character_spacing: 0.0, // PDF spec default
            word_spacing: 0.0, // PDF spec default
            rise: 0.0,      // PDF spec default
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
}

impl CanvasState<'_> {
    /// Default line width in user space units.
    /// PDF 1.7 Specification, Section 8.4.3.2 "Line Width", states the default value is 1.0.
    const DEFAULT_LINE_WIDTH: f32 = 1.0;
    /// Default fill color.
    /// PDF 1.7 Specification, Section 8.6.4 "Color Spaces", states the initial nonstroking color
    /// is black in the DeviceGray color space. This is equivalent to (0.0, 0.0, 0.0) in DeviceRGB.
    const DEFAULT_FILL_COLOR: Color = Color::from_rgb(0.0, 0.0, 0.0);
    /// Default stroke color.
    /// PDF 1.7 Specification, Section 8.6.4 "Color Spaces", states the initial stroking color
    /// is black in the DeviceGray color space. This is equivalent to (0.0, 0.0, 0.0) in DeviceRGB.
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
        }
    }
}

pub struct PdfCanvas<'a> {
    pub(crate) current_path: Option<PdfPath>,
    pub(crate) canvas: &'a mut dyn CanvasBackend,
    pub(crate) page: &'a PdfPage,
    // canvas_stack stores the graphics states, including text state.
    canvas_stack: Vec<CanvasState<'a>>,
}

impl<'a> PdfCanvas<'a> {
    pub fn new(backend: &'a mut dyn CanvasBackend, page: &'a PdfPage) -> Self {
        let media_box = &page.media_box;

        // Use descriptive names and ensure f32 type for PDF dimensions.
        // The `as f32` cast assumes media_box.width/height might return non-f32 types.
        let pdf_media_width = media_box.width() as f32;
        let pdf_media_height = media_box.height() as f32;

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

        // Initialize TextState with application-specific defaults if different from PDF spec.
        // For example, PdfCanvas used an initial font size of 18.0.
        let initial_text_state = TextState {
            font_size: 18.0, // Application-specific default for initial state
            ..TextState::default()
        };

        let canvas_stack = vec![CanvasState {
            transform: userspace_matrix,
            text_state: initial_text_state,
            ..Default::default()
        }];

        Self {
            current_path: None,
            canvas: backend,
            page,
            canvas_stack,
        }
    }

    pub(crate) fn map_point(&self, x: f32, y: f32) -> (f32, f32) {
        self.current_state().transform.transform_point(x, y)
    }
    pub(crate) fn current_state(&self) -> &CanvasState<'a> {
        self.canvas_stack.last().unwrap()
    }

    pub(crate) fn current_state_mut(&mut self) -> &mut CanvasState<'a> {
        self.canvas_stack.last_mut().unwrap()
    }

    pub(crate) fn save(&mut self) {
        let state = self.current_state().clone();
        self.canvas_stack.push(state);
    }

    pub(crate) fn restore(&mut self) {
        self.canvas_stack.pop();
    }

    pub(crate) fn paint_taken_path(
        &mut self,
        mode: PaintMode,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        if let Some(path) = self.current_path.take() {
            if mode == PaintMode::Fill {
                self.canvas
                    .fill_path(&path, fill_type, self.current_state().fill_color);
            } else {
                self.canvas.stroke_path(
                    &path,
                    self.current_state().stroke_color,
                    self.current_state().line_width,
                );
            }
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }
}
