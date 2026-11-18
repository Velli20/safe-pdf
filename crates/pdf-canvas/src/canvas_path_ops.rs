use pdf_graphics::pdf_path::PdfPath;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Handles `m`: begins a new subpath at (x, y).
    pub(crate) fn move_to(&mut self, x: f32, y: f32) -> Result<(), PdfCanvasError> {
        self.current_path
            .get_or_insert_with(PdfPath::default)
            .move_to(x, y);
        Ok(())
    }

    /// Handles `l`: appends a line segment to (x, y).
    pub(crate) fn line_to(&mut self, x: f32, y: f32) -> Result<(), PdfCanvasError> {
        self.current_path
            .get_or_insert_with(PdfPath::default)
            .line_to(x, y);
        Ok(())
    }

    /// Handles `c`: appends a cubic Bézier curve.
    pub(crate) fn curve_to(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    ) -> Result<(), PdfCanvasError> {
        self.current_path
            .get_or_insert_with(PdfPath::default)
            .curve_to(x1, y1, x2, y2, x3, y3);
        Ok(())
    }

    /// Handles `v`: appends a Bézier curve whose first control point is the current point.
    pub(crate) fn curve_to_v(
        &mut self,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    ) -> Result<(), PdfCanvasError> {
        let path = self.current_path.get_or_insert_with(PdfPath::default);
        if let Some((x, y)) = path.current_point() {
            path.curve_to(x, y, x2, y2, x3, y3);
            Ok(())
        } else {
            Err(PdfCanvasError::CurrentPointRequired)
        }
    }

    /// Handles `y`: appends a Bézier curve whose second control point is its end point.
    pub(crate) fn curve_to_y(
        &mut self,
        x1: f32,
        y1: f32,
        x3: f32,
        y3: f32,
    ) -> Result<(), PdfCanvasError> {
        self.current_path
            .get_or_insert_with(PdfPath::default)
            .curve_to(x1, y1, x3, y3, x3, y3);
        Ok(())
    }

    /// Handles `h`: closes the current subpath.
    pub(crate) fn close_path(&mut self) -> Result<(), PdfCanvasError> {
        self.current_path
            .get_or_insert_with(PdfPath::default)
            .close();
        Ok(())
    }

    /// Handles `re`: appends a closed rectangle subpath.
    pub(crate) fn rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Result<(), PdfCanvasError> {
        let path = self.current_path.get_or_insert_with(PdfPath::default);

        path.move_to(x, y);
        path.line_to(x + width, y);
        path.line_to(x + width, y + height);
        path.line_to(x, y + height);
        path.close();
        Ok(())
    }
}
