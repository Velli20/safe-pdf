use error::PdfCanvasError;
use pdf_canvas::PdfCanvas;
use pdf_content_stream::pdf_operator_backend::{
    PdfOperatorBackend, PdfOperatorBackendError, ShadingOps,
};

use crate::canvas_backend::CanvasBackend;

mod canvas;
pub mod canvas_backend;
mod canvas_clip_ops;
mod canvas_color_ops;
mod canvas_external_object_ops;
mod canvas_graphics_state_ops;
mod canvas_marked_content_ops;
mod canvas_path_ops;
mod canvas_state;
mod canvas_text_ops;
mod error;
pub mod pdf_canvas;
mod text_renderer;
mod text_state;
mod truetype_font_renderer;
mod type3_font_renderer;

impl<U, T: CanvasBackend<ImageType = U>> PdfOperatorBackend for PdfCanvas<'_, T, U> {}

impl<U, T: CanvasBackend<ImageType = U>> ShadingOps for PdfCanvas<'_, T, U> {
    fn paint_shading(&mut self, _shading_name: &str) -> Result<(), Self::ErrorType> {
        Err(PdfCanvasError::NotImplemented("paint_shading".into()))
    }
}

impl<T, U> PdfOperatorBackendError for PdfCanvas<'_, T, U> {
    type ErrorType = PdfCanvasError;
}
