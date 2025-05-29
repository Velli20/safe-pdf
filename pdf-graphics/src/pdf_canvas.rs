use pdf_font::font::Font;
use pdf_page::page::PdfPage;
use ttf_parser::Face;

use crate::{CanvasBackend, pdf_path::PdfPath, transform::Transform};

pub struct PdfCanvas<'a> {
    pub(crate) current_path: Option<PdfPath>,
    pub(crate) canvas: &'a mut dyn CanvasBackend,
    pub(crate) page: &'a PdfPage,
    pub(crate) current_font: Option<&'a Font>,
    pub(crate) font_face: Option<Face<'a>>,
    canvas_stack: Vec<CanvasState>,

    pub(crate) text_matrix: Transform,
    pub(crate) text_line_matrix: Transform,
    text_rendering_matrix: Transform,
    pub(crate) text_horizontal_scaling: f32,
    pub(crate) text_font_size: f32,
    pub(crate) text_character_spacing: f32,
    pub(crate) text_word_spacing: f32,
    pub(crate) text_rise: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct CanvasState {
    pub transform: Transform,
}

impl<'a> PdfCanvas<'a> {
    pub fn new(backend: &'a mut dyn CanvasBackend, page: &'a PdfPage) -> Self {
        let mut userspace_matrix = Transform::identity();
        let media_box = &page.media_box;

        let width = media_box.width();
        let height = media_box.height();
        let scale_x = backend.width() / width as f32;
        let scale_y = backend.height() / height as f32;
        userspace_matrix.scale(scale_x, scale_y);

        // PDF user-space coordinate y axis increases from bottom to top, so we have to
        // insert a horizontal reflection about the vertical midpoint into our transformation
        // matrix

        const VERTICAL_REFLECTION_MATRIX: Transform =
            Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, 0.0);
        userspace_matrix.concat(&VERTICAL_REFLECTION_MATRIX);
        userspace_matrix.translate(0.0, height as f32);

        let canvas_stack = vec![CanvasState {
            transform: userspace_matrix,
        }];

        Self {
            current_path: None,
            canvas: backend,
            page,
            current_font: None,
            font_face: None,
            canvas_stack,
            text_matrix: Transform::identity(),
            text_rendering_matrix: Transform::identity(),
            text_line_matrix: Transform::identity(),
            text_horizontal_scaling: 1.0,
            text_font_size: 1.0,
            text_rise: 0.0,
            text_character_spacing: 0.0,
            text_word_spacing: 0.0,
        }
    }

    pub(crate) fn map_point(&self, x: f32, y: f32) -> (f32, f32) {
        self.current_state().transform.transform_point(x, y)
    }

    pub(crate) fn current_state(&self) -> &CanvasState {
        self.canvas_stack.last().unwrap()
    }

    pub(crate) fn save(&mut self) {
        self.canvas_stack.push(self.current_state().clone());
    }

    pub(crate) fn restore(&mut self) {
        self.canvas_stack.pop();
    }

    pub(crate) fn text_rendering_matrix(&self) -> Transform {
        // PDF 1.7, 5.3.3. Text Space Details
        let parameter_matrix = Transform::from_row(
            self.text_horizontal_scaling,
            0.0,
            0.0,
            1.0,
            0.0,
            self.text_rise,
        );
        let mut text_rendering_matrix = self.current_state().transform;
        text_rendering_matrix.concat(&self.text_matrix);
        text_rendering_matrix.concat(&parameter_matrix);

        text_rendering_matrix
    }
}
