use crate::canvas_backend::CanvasBackend;
use crate::error::PdfCanvasError;
use crate::pdf_canvas::PdfCanvas;
use pdf_graphics::pdf_path::PdfPath;
use pdf_graphics::transform::Transform;
use pdf_graphics::{PaintMode, PathFillType};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlineGlyph, OutlinePen};

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

/// Draws a single outline glyph onto `canvas` using `PdfPathPen`.
///
/// If the outline cannot be drawn (e.g. an empty glyph), the call is silently
/// skipped — missing glyphs are not an error.
pub(crate) fn draw_outline_glyph<B: CanvasBackend>(
    canvas: &mut PdfCanvas<'_, B>,
    outline_glyph: &OutlineGlyph<'_>,
    size: Size,
    transform: &Transform,
) -> Result<(), PdfCanvasError> {
    let mut pen = PdfPathPen::default();
    let settings = DrawSettings::from((size, LocationRef::default()));
    if outline_glyph.draw(settings, &mut pen).is_ok() {
        pen.path.transform(transform);
        canvas.draw_path(&pen.path, PaintMode::Fill, PathFillType::Winding)?;
    }
    Ok(())
}
