use pdf_content_stream::pdf_operator_backend::{PathConstructionOps, PathPaintingOps};

use crate::{
    PaintMode, PathFillType, error::PdfCanvasError, pdf_canvas::PdfCanvas, pdf_path::PdfPath,
};

impl<'a> PathConstructionOps for PdfCanvas<'a> {
    fn move_to(&mut self, x: f32, y: f32) -> Result<(), Self::ErrorType> {
        self.current_path
            .get_or_insert(PdfPath::default())
            .move_to(x, y);
        Ok(())
    }

    fn line_to(&mut self, x: f32, y: f32) -> Result<(), Self::ErrorType> {
        self.current_path
            .get_or_insert(PdfPath::default())
            .line_to(x, y);
        Ok(())
    }

    fn curve_to(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    ) -> Result<(), Self::ErrorType> {
        self.current_path
            .get_or_insert(PdfPath::default())
            .curve_to(x1, y1, x2, y2, x3, y3);
        Ok(())
    }

    fn curve_to_v(&mut self, x2: f32, y2: f32, x3: f32, y3: f32) -> Result<(), Self::ErrorType> {
        let path = self.current_path.get_or_insert(PdfPath::default());
        if let Some((x, y)) = path.current_point() {
            path.curve_to(x, y, x2, y2, x3, y3);
            Ok(())
        } else {
            Err(PdfCanvasError::NoCurrentPoint)
        }
    }

    fn curve_to_y(&mut self, x1: f32, y1: f32, x3: f32, y3: f32) -> Result<(), Self::ErrorType> {
        self.current_path
            .get_or_insert(PdfPath::default())
            .curve_to(x1, y1, x3, y3, x3, y3);
        Ok(())
    }

    fn close_path(&mut self) -> Result<(), Self::ErrorType> {
        self.current_path.get_or_insert(PdfPath::default()).close();
        Ok(())
    }

    fn rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Result<(), Self::ErrorType> {
        let path = self.current_path.get_or_insert(PdfPath::default());

        path.move_to(x, y);
        path.line_to(x + width, y);
        path.line_to(x + width, y + height);
        path.line_to(x, y + height);
        path.close();
        Ok(())
    }
}

impl<'a> PathPaintingOps for PdfCanvas<'a> {
    fn stroke_path(&mut self) -> Result<(), Self::ErrorType> {
        self.paint_taken_path(PaintMode::Stroke, PathFillType::default())
    }

    fn close_and_stroke_path(&mut self) -> Result<(), Self::ErrorType> {
        self.close_path()?;
        self.stroke_path()
    }

    fn fill_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.paint_taken_path(PaintMode::Fill, PathFillType::Winding)
    }

    fn fill_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.paint_taken_path(PaintMode::Fill, PathFillType::EvenOdd)
    }

    fn fill_and_stroke_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.paint_taken_path(PaintMode::FillAndStroke, PathFillType::Winding)
    }

    fn fill_and_stroke_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.paint_taken_path(PaintMode::FillAndStroke, PathFillType::EvenOdd)
    }

    fn close_fill_and_stroke_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.close_path()?;
        self.fill_and_stroke_path_nonzero_winding()
    }

    fn close_fill_and_stroke_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.close_path()?;
        self.fill_and_stroke_path_even_odd()
    }

    fn end_path_no_op(&mut self) -> Result<(), Self::ErrorType> {
        // Discard the current path, making it undefined.
        self.current_path.take();
        Ok(())
    }
}
