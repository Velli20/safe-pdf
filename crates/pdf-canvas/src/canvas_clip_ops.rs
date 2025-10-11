use pdf_content_stream::pdf_operator_backend::ClippingPathOps;
use pdf_graphics::PathFillType;

use crate::{error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<T: std::error::Error> ClippingPathOps for PdfCanvas<'_, T> {
    fn clip_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        let Some(path) = self.current_path.take() else {
            return Err(PdfCanvasError::NoActivePath);
        };
        self.set_clip_path(path, PathFillType::Winding)
    }

    fn clip_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        let Some(path) = self.current_path.take() else {
            return Err(PdfCanvasError::NoActivePath);
        };
        self.set_clip_path(path, PathFillType::EvenOdd)
    }
}
