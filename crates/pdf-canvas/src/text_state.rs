use pdf_font::PdfFontSpec;
use pdf_font::pdf_font_handle::PdfFontHandle;
use pdf_graphics::transform::Transform;
use pdf_resources::resources::Resources;
use pdf_text_engine::text_style::TextStyle;

/// PDF text state retained by the canvas around engine layout calls.
#[derive(Clone)]
pub(crate) struct TextState<'a> {
    pub(crate) matrix: Transform,
    pub(crate) line_matrix: Transform,
    pub(crate) style: TextStyle,
    pub(crate) leading: f32,
    pub(crate) font: Option<PdfFontHandle>,
    pub(crate) font_spec: Option<&'a PdfFontSpec>,
    pub(crate) resources: Option<&'a Resources>,
}

impl Default for TextState<'_> {
    fn default() -> Self {
        Self {
            matrix: Transform::identity(),
            line_matrix: Transform::identity(),
            style: TextStyle::default(),
            leading: 0.0,
            font: None,
            font_spec: None,
            resources: None,
        }
    }
}

impl TextState<'_> {
    pub(crate) fn move_line_position(&mut self, tx: f32, ty: f32) {
        self.line_matrix
            .post_concat(&Transform::from_translate(tx, ty));
        self.matrix = self.line_matrix;
    }

    pub(crate) fn set_matrices(&mut self, transform: Transform) {
        self.line_matrix = transform;
        self.matrix = transform;
    }

    pub(crate) fn move_to_next_line(&mut self) {
        self.move_line_position(0.0, -self.leading);
    }

    pub(crate) fn advance(&mut self, x: f32, y: f32) {
        self.matrix.post_translate(x, y);
    }
}
