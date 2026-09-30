use pdf_content_stream_operators::pdf_operator_backend::ClippingPathOps;
use pdf_graphics::PathFillType;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> ClippingPathOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn clip_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.pending_clip = Some(PathFillType::Winding);
        Ok(())
    }

    fn clip_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.pending_clip = Some(PathFillType::EvenOdd);
        Ok(())
    }
}
