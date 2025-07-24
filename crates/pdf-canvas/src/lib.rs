use error::PdfCanvasError;
use pdf_canvas::PdfCanvas;
use pdf_content_stream::pdf_operator_backend::{
    PdfOperatorBackend, PdfOperatorBackendError, ShadingOps,
};

use crate::canvas_backend::CanvasBackend;

pub mod canvas;
pub mod canvas_backend;
pub mod canvas_clip_ops;
pub mod canvas_color_ops;
pub mod canvas_external_object_ops;
pub mod canvas_graphics_state_ops;
pub mod canvas_marked_content_ops;
pub mod canvas_path_ops;
pub mod canvas_text_ops;
pub mod error;
pub mod pdf_canvas;
pub mod pdf_path;
pub mod text_renderer;
pub mod truetype_font_renderer;
pub mod type3_font_renderer;

#[derive(Default, Clone, PartialEq)]
pub enum PaintMode {
    #[default]
    Fill,
    Stroke,
    FillAndStroke,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PathFillType {
    /// Specifies that "inside" is computed by a non-zero sum of signed edge crossings
    #[default]
    Winding,
    /// Specifies that "inside" is computed by an odd number of edge crossings
    EvenOdd,
}

impl<'a, T: CanvasBackend> PdfOperatorBackend for PdfCanvas<'a, T> {}

impl<'a, T: CanvasBackend> ShadingOps for PdfCanvas<'a, T> {
    fn paint_shading(&mut self, shading_name: &str) -> Result<(), Self::ErrorType> {
        println!("Paint shading {:?}", shading_name);
        Ok(())
    }
}

impl<T> PdfOperatorBackendError for PdfCanvas<'_, T> {
    type ErrorType = PdfCanvasError;
}
