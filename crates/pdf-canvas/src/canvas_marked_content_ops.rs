use pdf_content_stream_operators::pdf_operator_backend::MarkedContentOps;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> MarkedContentOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn mark_point(&mut self, _tag: &[u8]) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn mark_point_with_properties(
        &mut self,
        _tag: &[u8],
        _properties_name_or_dict: &[u8],
    ) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn begin_marked_content(&mut self, _tag: &[u8]) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn begin_marked_content_with_properties(&mut self, _tag: &[u8]) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn end_marked_content(&mut self) -> Result<(), Self::ErrorType> {
        Ok(())
    }
}
