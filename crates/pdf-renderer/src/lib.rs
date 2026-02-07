use pdf_canvas::{
    canvas_backend::CanvasBackend, pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas,
};
use pdf_document::document::PdfDocument;
use thiserror::Error;

pub mod page_cache;

pub use page_cache::PageRecordingCache;

/// Errors that can occur while rendering a PDF document onto a canvas backend.
#[derive(Debug, Error)]
pub enum PdfRendererError {
    #[error("Page not found: {0}")]
    PageNotFound(usize),
    #[error("PDF canvas error: {0}")]
    PdfCanvasError(#[from] pdf_canvas::error::PdfCanvasError),
    #[error("Recording canvas error: {0}")]
    RecordingError(#[from] pdf_canvas::recording_canvas::RecordingCanvasError),
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
    /// rendered sequentially by repeatedly calling [`PdfRenderer::render`].
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
        let Some(page) = self.document.pages.get(page_index) else {
            return Err(PdfRendererError::PageNotFound(page_index));
        };
        let mut canvas = PdfCanvas::new(self.canvas, page, None)?;
        if let Some(cs) = &page.contents {
            canvas.render_content_stream(
                &cs.operations,
                None,
                None,
                page.resources.as_ref(),
                None,
            )?;
        }
        Ok(())
    }
}

/// Renders a PDF page into a [`RecordingCanvas`] for caching.
///
/// This function records all drawing commands for a page into a resolution-independent
/// `RecordingCanvas` that can be replayed to any backend at any size.
///
/// # Parameters
///
/// - `document`: The PDF document containing the page.
/// - `page_index`: Zero-based index of the page to render.
/// - `width`: Logical width of the recording canvas.
/// - `height`: Logical height of the recording canvas.
///
/// # Returns
///
/// A `RecordingCanvas` containing all drawing commands for the page.
///
/// # Errors
///
/// Returns an error if the page is not found or if rendering fails.
pub fn render_page_to_recording(
    document: &PdfDocument,
    page_index: usize,
    width: f32,
    height: f32,
) -> Result<RecordingCanvas, PdfRendererError> {
    let Some(page) = document.pages.get(page_index) else {
        return Err(PdfRendererError::PageNotFound(page_index));
    };

    let mut recording = RecordingCanvas::new(width, height);
    {
        let mut canvas = PdfCanvas::new(&mut recording, page, None)?;
        if let Some(cs) = &page.contents {
            canvas.render_content_stream(
                &cs.operations,
                None,
                None,
                page.resources.as_ref(),
                None,
            )?;
        }
    }

    Ok(recording)
}

/// Renders a cached [`RecordingCanvas`] to a backend, or renders the page
/// directly if not cached.
///
/// This is a convenience function that handles the cache lookup and fallback
/// rendering in a single call.
///
/// # Parameters
///
/// - `document`: The PDF document containing the page.
/// - `page_index`: Zero-based index of the page to render.
/// - `cache`: The page recording cache.
/// - `backend`: The canvas backend to render to.
///
/// # Returns
///
/// Returns `Ok(())` if rendering was successful.
///
/// # Errors
///
/// Returns an error if the page is not found or if rendering fails.
pub fn render_page_cached<B: CanvasBackend>(
    document: &PdfDocument,
    page_index: usize,
    cache: &mut PageRecordingCache,
    backend: &mut B,
) -> Result<(), PdfRendererError> {
    let width = backend.width();
    let height = backend.height();

    // Check cache first
    if let Some(recording) = cache.get(page_index) {
        // Clone to avoid borrow issues, then replay
        recording.replay(backend).map_err(|e| {
            PdfRendererError::PdfCanvasError(pdf_canvas::error::PdfCanvasError::BackendError(
                e.to_string(),
            ))
        })?;
        return Ok(());
    }

    // Cache miss: render to recording canvas
    let recording = render_page_to_recording(document, page_index, width, height)?;

    // Replay to the actual backend
    recording.replay(backend).map_err(|e| {
        PdfRendererError::PdfCanvasError(pdf_canvas::error::PdfCanvasError::BackendError(
            e.to_string(),
        ))
    })?;

    // Store in cache for next time
    cache.insert(page_index, recording);

    Ok(())
}
