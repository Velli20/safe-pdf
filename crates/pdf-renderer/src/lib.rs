use pdf_canvas::{canvas_backend::CanvasBackend, pdf_canvas::PdfCanvas};
use pdf_document::PdfDocument;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfRendererError {
    #[error("Page not found: {0}")]
    PageNotFound(usize),
    #[error("PDF canvas error: {0}")]
    PdfCanvasError(#[from] pdf_canvas::error::PdfCanvasError),
}
/// A PDF renderer that draws PDF pages onto a canvas backend.
pub struct PdfRenderer<'a, 'b, T> {
    document: &'b PdfDocument,
    canvas: &'a mut dyn CanvasBackend<MaskType = T>,
}

impl<'a, 'b, T: CanvasBackend> PdfRenderer<'a, 'b, T> {
    pub fn new(document: &'b PdfDocument, canvas: &'a mut dyn CanvasBackend<MaskType = T>) -> Self {
        Self { document, canvas }
    }

    pub fn render(&mut self, page_index: usize) -> Result<(), PdfRendererError> {
        let Some(p) = self.document.pages.get(page_index) else {
            return Err(PdfRendererError::PageNotFound(page_index));
        };
        let mut canvas = PdfCanvas::new(self.canvas, p, None)?;

        if let Some(cs) = &p.contents {
            canvas.render_content_stream(&cs.operations, None, None)?;
        }
        Ok(())
    }
}
