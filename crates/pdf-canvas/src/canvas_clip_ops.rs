use pdf_content_stream::pdf_operator_backend::ClippingPathOps;

use crate::{
    PathFillType, canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
};

impl<'a, U, T: CanvasBackend<ImageType = U>> ClippingPathOps for PdfCanvas<'a, T, U> {
    fn clip_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        if let Some(mut path) = self.current_path.take() {
            path.transform(&self.current_state()?.transform);
            if self.current_state()?.clip_path.is_some() {
                self.canvas.reset_clip();
            }

            self.canvas.set_clip_region(&path, PathFillType::Winding);
            self.current_state_mut()?.clip_path = Some(path);
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }

    fn clip_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        if let Some(mut path) = self.current_path.take() {
            path.transform(&self.current_state()?.transform);
            if self.current_state()?.clip_path.is_some() {
                self.canvas.reset_clip();
            }

            self.canvas.set_clip_region(&path, PathFillType::EvenOdd);
            self.current_state_mut()?.clip_path = Some(path);
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }
}
