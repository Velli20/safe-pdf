use pdf_graphics::{PaintMode, PathFillType, pdf_path::PdfPath};

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

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Handles `S`: strokes the current path.
    pub(crate) fn stroke_path(&mut self) -> Result<(), PdfCanvasError> {
        self.end_path(Some((PaintMode::Stroke, PathFillType::default())))
    }

    /// Handles `s`: closes and strokes the current path.
    pub(crate) fn close_and_stroke_path(&mut self) -> Result<(), PdfCanvasError> {
        self.close_path()?;
        self.stroke_path()
    }

    /// Handles `f`/`F`: fills the current path with the nonzero winding rule.
    pub(crate) fn fill_path_nonzero_winding(&mut self) -> Result<(), PdfCanvasError> {
        self.end_path(Some((PaintMode::Fill, PathFillType::Winding)))
    }

    /// Handles `f*`: fills the current path with the even-odd rule.
    pub(crate) fn fill_path_even_odd(&mut self) -> Result<(), PdfCanvasError> {
        self.end_path(Some((PaintMode::Fill, PathFillType::EvenOdd)))
    }

    /// Handles `B`: fills (nonzero winding) and strokes the current path.
    pub(crate) fn fill_and_stroke_path_nonzero_winding(&mut self) -> Result<(), PdfCanvasError> {
        self.end_path(Some((PaintMode::FillAndStroke, PathFillType::Winding)))
    }

    /// Handles `B*`: fills (even-odd) and strokes the current path.
    pub(crate) fn fill_and_stroke_path_even_odd(&mut self) -> Result<(), PdfCanvasError> {
        self.end_path(Some((PaintMode::FillAndStroke, PathFillType::EvenOdd)))
    }

    /// Handles `b`: closes, fills (nonzero winding) and strokes the current path.
    pub(crate) fn close_fill_and_stroke_path_nonzero_winding(
        &mut self,
    ) -> Result<(), PdfCanvasError> {
        self.close_path()?;
        self.fill_and_stroke_path_nonzero_winding()
    }

    /// Handles `b*`: closes, fills (even-odd) and strokes the current path.
    pub(crate) fn close_fill_and_stroke_path_even_odd(&mut self) -> Result<(), PdfCanvasError> {
        self.close_path()?;
        self.fill_and_stroke_path_even_odd()
    }

    /// Handles `n`: ends the current path without painting it.
    pub(crate) fn end_path_no_op(&mut self) -> Result<(), PdfCanvasError> {
        self.end_path(None)
    }
}
