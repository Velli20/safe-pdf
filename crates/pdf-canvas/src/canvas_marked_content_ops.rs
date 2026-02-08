use pdf_content_stream::pdf_operator_backend::MarkedContentOps;

use crate::pdf_canvas::PdfCanvas;

impl MarkedContentOps for PdfCanvas<'_> {
    fn mark_point(&mut self, _tag: &str) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn mark_point_with_properties(
        &mut self,
        _tag: &str,
        _properties_name_or_dict: &str,
    ) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn begin_marked_content(&mut self, _tag: &str) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn begin_marked_content_with_properties(&mut self, _tag: &str) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn end_marked_content(&mut self) -> Result<(), Self::ErrorType> {
        Ok(())
    }
}
