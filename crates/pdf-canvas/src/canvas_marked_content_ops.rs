use std::rc::Rc;

use pdf_content_stream::pdf_operator_backend::MarkedContentOps;
use pdf_object::dictionary::Dictionary;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<T: CanvasBackend> MarkedContentOps for PdfCanvas<'_, T> {
    fn mark_point(&mut self, _tag: &str) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented("mark_point".into()))
    }

    fn mark_point_with_properties(
        &mut self,
        _tag: &str,
        _properties_name_or_dict: &str,
    ) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented(
            "mark_point_with_properties".into(),
        ))
    }

    fn begin_marked_content(&mut self, _tag: &str) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented(
            "begin_marked_content".into(),
        ))
    }

    fn begin_marked_content_with_properties(
        &mut self,
        _tag: &str,
        _properties: &Rc<Dictionary>,
    ) -> Result<(), Self::ErrorType> {
        // Err(PdfCanvasError::NotImplemented(
        //     "begin_marked_content_with_properties".into(),
        // ))
        Ok(())
    }

    fn end_marked_content(&mut self) -> Result<(), Self::ErrorType> {
        Ok(())
    }
}
