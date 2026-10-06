use pdf_content_stream_operators::pdf_operator_backend::ClippingPathOps;
use pdf_graphics::PathFillType;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> ClippingPathOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn clip_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.mark_clip_path(PathFillType::Winding)
    }

    fn clip_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.mark_clip_path(PathFillType::EvenOdd)
    }
}

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Marks the current path for clipping with `rule`; the clip is applied when the next
    /// path-painting operator ends the path.
    fn mark_clip_path(&mut self, rule: PathFillType) -> Result<(), PdfCanvasError> {
        if self.current_path.is_none() {
            return Err(PdfCanvasError::PathRequired);
        }
        self.pending_clip = Some(rule);
        Ok(())
    }
}
