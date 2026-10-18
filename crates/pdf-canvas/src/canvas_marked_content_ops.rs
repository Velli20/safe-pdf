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

    fn begin_marked_content(&mut self, tag: &[u8]) -> Result<(), Self::ErrorType> {
        if let Some(filter) = &mut self.optional_content {
            filter.begin(tag, || None);
        }
        Ok(())
    }

    fn begin_marked_content_with_properties(
        &mut self,
        tag: &[u8],
        properties: Option<&[u8]>,
    ) -> Result<(), Self::ErrorType> {
        let Some(filter) = &mut self.optional_content else {
            return Ok(());
        };
        // Optional content the resources do not declare stays visible.
        let resources = self
            .canvas_stack
            .last()
            .and_then(|state| state.resources.as_deref());
        filter.begin(tag, || resources?.optional_content(properties?));
        Ok(())
    }

    fn end_marked_content(&mut self) -> Result<(), Self::ErrorType> {
        if let Some(filter) = &mut self.optional_content {
            filter.end();
        }
        Ok(())
    }
}
