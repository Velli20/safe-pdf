use pdf_graphics::pdf_path::PdfPath;
use skrifa::outline::OutlinePen;

/// An implementation of `skrifa::outline::OutlinePen` that converts glyph outlines
/// into a `PdfPath`.
#[derive(Default)]
pub(crate) struct PdfPathPen {
    /// The `PdfPath` being constructed from the glyph outline commands.
    pub path: PdfPath,
}

impl OutlinePen for PdfPathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.curve_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.path.close();
    }
}
