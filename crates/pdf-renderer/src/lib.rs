use pdf_canvas::{
    canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
    recording_canvas::RecordingCanvas,
};
use pdf_document::document::PdfDocument;
use pdf_text_engine::FontSystem;
use std::sync::Arc;
use thiserror::Error;

mod document_text_selection;
pub mod page_cache;
pub mod text_selection;

pub use page_cache::PageRecordingCache;
pub use text_selection::{
    DocumentTextSelection, PageTextLayout, SelectionBatch, SelectionPoint, SelectionSpan,
    TextGlyph, TextHit, TextSelection, TextSelectionError, TextSelectionResult,
};

/// Renders page content and captures text using a shared viewport, excluding annotations.
/// Backend lifecycle and any separate annotation pass remain the caller's responsibility.
pub fn render_page_content_with_text_layout<B: CanvasBackend>(
    backend: &mut B,
    page: &pdf_document::page::PdfPage,
    viewport: &pdf_canvas::PageViewport,
    fonts: Arc<FontSystem>,
) -> Result<PageTextLayout, PdfCanvasError> {
    let mut canvas =
        PdfCanvas::new_with_viewport(backend, page, viewport, fonts)?.with_text_recording();
    if let Some(content) = &page.contents {
        canvas.render_content_stream(content, None, None, page.resources.clone(), None)?;
    }
    Ok(PageTextLayout::new(canvas.take_text_glyphs()))
}

/// A page's drawing commands and selectable text captured in one render pass.
#[derive(Clone)]
pub struct RecordedPage {
    recording: RecordingCanvas,
    text_layout: Arc<PageTextLayout>,
}

impl RecordedPage {
    /// Returns the recorded drawing commands.
    pub fn recording(&self) -> &RecordingCanvas {
        &self.recording
    }

    /// Returns the selectable text layout captured with the drawing commands.
    pub fn text_layout(&self) -> &PageTextLayout {
        &self.text_layout
    }

    /// Shares the text layout with a selection controller without copying glyphs.
    pub fn text_layout_arc(&self) -> Arc<PageTextLayout> {
        Arc::clone(&self.text_layout)
    }

    /// Replays this page onto a concrete backend.
    pub fn replay<B: CanvasBackend>(&self, backend: &mut B) -> Result<(), PdfCanvasError> {
        self.recording.replay(backend)
    }

    /// Consumes this page and returns its drawing commands and text layout.
    pub fn into_parts(self) -> (RecordingCanvas, Arc<PageTextLayout>) {
        (self.recording, self.text_layout)
    }
}

/// Errors that can occur while rendering a PDF document onto a canvas backend.
#[derive(Debug, Error)]
pub enum PdfRendererError {
    #[error("Page not found: {0}")]
    PageNotFound(usize),
    #[error("PDF canvas error: {0}")]
    PdfCanvasError(#[from] pdf_canvas::error::PdfCanvasError),
}

/// Renders pages of a [`PdfDocument`] onto a user supplied [`CanvasBackend`].
pub struct PdfRenderer {
    document: PdfDocument,
    font_system: Arc<FontSystem>,
}

impl PdfRenderer {
    /// Creates a new renderer over the owned PDF `document`.
    ///
    /// The renderer owns the document for its lifetime. Call [`PdfRenderer::render`]
    /// with a mutable canvas backend each time a page should be drawn.
    pub fn new(document: PdfDocument) -> Self {
        Self {
            document,
            font_system: pdf_text_engine::bundled_font_system(),
        }
    }

    /// Creates a renderer using a caller-provided shared font system.
    pub fn new_with_font_system(document: PdfDocument, font_system: Arc<FontSystem>) -> Self {
        Self {
            document,
            font_system,
        }
    }

    /// Returns the owned document by reference.
    pub fn document(&self) -> &PdfDocument {
        &self.document
    }

    /// Returns the owned document mutably for in-memory editing.
    pub fn document_mut(&mut self) -> &mut PdfDocument {
        &mut self.document
    }

    /// Returns the owned document after rendering is complete.
    pub fn into_document(self) -> PdfDocument {
        self.document
    }

    /// Renders page content only; native annotations are presented separately.
    ///
    /// # Parameters
    ///
    /// - `page_index` – Zero-based index of the page to render.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the page was rendered successfully, or an error if the
    /// page could not be found or if an error occurred during rendering.
    pub fn render<B: CanvasBackend>(
        &self,
        canvas_backend: &mut B,
        page_index: usize,
    ) -> Result<(), PdfRendererError> {
        let page = self.page(page_index)?;
        {
            let mut canvas =
                PdfCanvas::new(canvas_backend, page, None, Arc::clone(&self.font_system))?;
            if let Some(cs) = &page.contents {
                canvas.render_content_stream(cs, None, None, page.resources.clone(), None)?;
            }
        }
        Ok(())
    }

    /// Renders a page and returns selectable text captured during the same pass.
    pub fn render_with_text_layout<B: CanvasBackend>(
        &self,
        canvas_backend: &mut B,
        page_index: usize,
    ) -> Result<PageTextLayout, PdfRendererError> {
        let page = self.page(page_index)?;
        let mut canvas = PdfCanvas::new(canvas_backend, page, None, Arc::clone(&self.font_system))?
            .with_text_recording();
        if let Some(cs) = &page.contents {
            canvas.render_content_stream(cs, None, None, page.resources.clone(), None)?;
        }
        let glyphs = canvas.take_text_glyphs();
        Ok(PageTextLayout::new(glyphs))
    }

    /// Records page content and text for caching, excluding native annotations.
    ///
    /// Drawing commands and glyph bounds use the requested device dimensions;
    /// replay the result only onto a backend with the same dimensions.
    pub fn render_page_to_recording(
        &self,
        page_index: usize,
        width: f32,
        height: f32,
    ) -> Result<RecordedPage, PdfRendererError> {
        let page = self.page(page_index)?;
        let mut recording = RecordingCanvas::new(width, height);
        let glyphs = {
            let mut canvas =
                PdfCanvas::new(&mut recording, page, None, Arc::clone(&self.font_system))?
                    .with_text_recording();
            if let Some(cs) = &page.contents {
                canvas.render_content_stream(cs, None, None, page.resources.clone(), None)?;
            }
            canvas.take_text_glyphs()
        };
        Ok(RecordedPage {
            recording,
            text_layout: Arc::new(PageTextLayout::new(glyphs)),
        })
    }

    fn page(&self, page_index: usize) -> Result<&pdf_document::page::PdfPage, PdfRendererError> {
        let Some(page) = self.document.pages.get(page_index) else {
            return Err(PdfRendererError::PageNotFound(page_index));
        };
        Ok(page)
    }
}

/// Renders a cached [`RecordedPage`] to a backend, or records the page
/// directly if not cached.
///
/// This is a convenience function that handles the cache lookup and fallback
/// rendering in a single call.
///
/// # Parameters
///
/// - `renderer`: The PDF renderer owning the document containing the page.
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
    renderer: &PdfRenderer,
    page_index: usize,
    cache: &mut PageRecordingCache,
    backend: &mut B,
) -> Result<(), PdfRendererError> {
    let width = backend.width();
    let height = backend.height();

    // Check cache first
    if let Some(recording) = cache.get(page_index, width, height) {
        // Replay directly from the cached recording.
        recording.replay(backend)?;
        return Ok(());
    }

    // Cache miss: render to recording canvas
    let recording = renderer.render_page_to_recording(page_index, width, height)?;

    // Replay to the actual backend
    recording.replay(backend)?;

    // Store in cache for next time
    cache.insert(page_index, recording);

    Ok(())
}
