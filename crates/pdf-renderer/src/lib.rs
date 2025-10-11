use pdf_canvas::{canvas_backend::CanvasBackend, pdf_canvas::PdfCanvas};
use pdf_document::PdfDocument;
use thiserror::Error;

/// Errors that can occur while rendering a PDF document onto a canvas backend.
#[derive(Debug, Error)]
pub enum PdfRendererError {
    #[error("Page not found: {0}")]
    PageNotFound(usize),
    #[error("PDF canvas error: {0}")]
    PdfCanvasError(#[from] pdf_canvas::error::PdfCanvasError),
}

/// Renders pages of a [`PdfDocument`] onto a user supplied [`CanvasBackend`].
///
/// Type Parameter:
///
/// - `T` – Mask type associated with the concrete `CanvasBackend` implementation.
pub struct PdfRenderer<'a, 'b, T> {
    document: &'b PdfDocument,
    canvas: &'a mut dyn CanvasBackend<ErrorType = T>,
}

impl<'a, 'b, T: std::error::Error> PdfRenderer<'a, 'b, T> {
    /// Creates a new renderer over the given PDF `document` and `canvas` backend.
    ///
    /// The caller retains ownership of the canvas; the renderer only holds a
    /// mutable borrow for the duration of its lifetime. Multiple pages can be
    /// rendered sequentially by repeatedly calling [`render`].
    pub fn new(
        document: &'b PdfDocument,
        canvas: &'a mut dyn CanvasBackend<ErrorType = T>,
    ) -> Self {
        Self { document, canvas }
    }

    /// Renders a page onto the canvas backend.
    ///
    /// # Parameters
    ///
    /// - `page_index` – Zero-based index of the page to render.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the page was rendered successfully, or an error if the
    /// page could not be found or if an error occurred during rendering.
    pub fn render(&mut self, page_index: usize) -> Result<(), PdfRendererError> {
        let Some(p) = self.document.pages.get(page_index) else {
            return Err(PdfRendererError::PageNotFound(page_index));
        };
        let mut canvas = PdfCanvas::new(self.canvas, p, None)?;
        println!("Rendering page {}", page_index + 1);
        if let Some(cs) = &p.contents {
            canvas.render_content_stream(&cs.operations, None, None)?;
        }
        Ok(())
    }
}
