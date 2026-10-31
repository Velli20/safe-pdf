use pdf_canvas::{
    PageViewport, canvas_backend::CanvasBackend, error::PdfCanvasError, image_cache::ImageCache,
    pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas,
};
use pdf_document::document::PdfDocument;
use pdf_object_reader::ObjectReadError;
use pdf_optional_content::OptionalContentState;
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
    PdfCanvasError(#[source] pdf_utils::TracedError<pdf_canvas::error::PdfCanvasError>),
    #[error("Failed to read page content stream: {0}")]
    ContentStream(#[from] ObjectReadError),
}

impl From<PdfCanvasError> for PdfRendererError {
    #[track_caller]
    fn from(error: PdfCanvasError) -> Self {
        Self::PdfCanvasError(pdf_utils::TracedError::new(error))
    }
}

impl PdfRendererError {
    /// Returns the backtrace captured when a canvas error entered the renderer.
    pub fn trace(&self) -> Option<&pdf_utils::ErrorTrace> {
        match self {
            Self::PdfCanvasError(error) => Some(error.trace()),
            Self::PageNotFound(_) | Self::ContentStream(_) => None,
        }
    }
}

/// Renders pages of a [`PdfDocument`] onto a user supplied [`CanvasBackend`].
pub struct PdfRenderer {
    document: PdfDocument,
    font_system: Arc<FontSystem>,
    /// Image rasters kept between renders, so drawing a page again does not decode
    /// its images again.
    image_cache: Arc<ImageCache>,
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
            image_cache: Arc::default(),
        }
    }

    /// Creates a renderer using a caller-provided shared font system.
    pub fn new_with_font_system(document: PdfDocument, font_system: Arc<FontSystem>) -> Self {
        Self {
            document,
            font_system,
            image_cache: Arc::default(),
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
        let optional_content = self.default_optional_content();
        {
            let mut canvas =
                PdfCanvas::new(canvas_backend, page, None, Arc::clone(&self.font_system))?
                    .with_image_cache(Arc::clone(&self.image_cache))
                    .with_optional_content(&optional_content);
            if let Some(cs) = self.document.content_stream(page)? {
                canvas.render_content_stream(&cs, None, None, page.resources.clone(), None)?;
            }
        }
        Ok(())
    }

    /// Renders page content and captures selectable text in the same pass, mapping the page
    /// through `viewport`. Annotations are excluded, and content in optional content groups
    /// that are off in `optional_content` is hidden.
    /// Backend lifecycle and any separate annotation pass remain the caller's responsibility.
    pub fn render_with_text_layout<B: CanvasBackend>(
        &self,
        canvas_backend: &mut B,
        page_index: usize,
        viewport: &PageViewport,
        optional_content: &OptionalContentState,
    ) -> Result<PageTextLayout, PdfRendererError> {
        let page = self.page(page_index)?;
        let mut canvas = PdfCanvas::new_with_viewport(
            canvas_backend,
            page,
            viewport,
            Arc::clone(&self.font_system),
        )?
        .with_image_cache(Arc::clone(&self.image_cache))
        .with_text_recording()
        .with_optional_content(optional_content);
        if let Some(cs) = self.document.content_stream(page)? {
            canvas.render_content_stream(&cs, None, None, page.resources.clone(), None)?;
        }
        Ok(PageTextLayout::new(canvas.take_text_glyphs()))
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
        let optional_content = self.default_optional_content();
        let glyphs = {
            let mut canvas =
                PdfCanvas::new(&mut recording, page, None, Arc::clone(&self.font_system))?
                    .with_image_cache(Arc::clone(&self.image_cache))
                    .with_text_recording()
                    .with_optional_content(&optional_content);
            if let Some(cs) = self.document.content_stream(page)? {
                canvas.render_content_stream(&cs, None, None, page.resources.clone(), None)?;
            }
            canvas.take_text_glyphs()
        };
        Ok(RecordedPage {
            recording,
            text_layout: Arc::new(PageTextLayout::new(glyphs)),
        })
    }

    /// Returns the visibility the document's default optional content configuration sets.
    pub fn default_optional_content(&self) -> OptionalContentState {
        self.document
            .optional_content
            .as_ref()
            .map(OptionalContentState::from_properties)
            .unwrap_or_default()
    }

    /// Returns the page at zero-based `page_index`, or [`PdfRendererError::PageNotFound`].
    pub fn page(
        &self,
        page_index: usize,
    ) -> Result<&pdf_document::page::PdfPage, PdfRendererError> {
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
