use pdf_annotations::{AnnotationRenderError, AnnotationRenderer};
use pdf_canvas::{
    canvas_backend::{CanvasBackend, Image, Shader},
    error::PdfCanvasError,
    pdf_canvas::PdfCanvas,
    recording_canvas::RecordingCanvas,
    stroke_style::StrokeStyle,
};
use pdf_document::document::PdfDocument;
use pdf_graphics::{
    BlendMode, MaskMode, PathFillType, color::Color, pdf_path::PdfPath, rect::Rect,
    transform::Transform,
};
use thiserror::Error;

pub mod page_cache;
pub mod text_selection;

pub use page_cache::PageRecordingCache;
pub use text_selection::PageTextLayout;

/// Errors that can occur while rendering a PDF document onto a canvas backend.
#[derive(Debug, Error)]
pub enum PdfRendererError {
    #[error("Page not found: {0}")]
    PageNotFound(usize),
    #[error("PDF canvas error: {0}")]
    PdfCanvasError(#[from] pdf_canvas::error::PdfCanvasError),
    #[error("Annotation render error: {0}")]
    AnnotationRenderError(#[from] AnnotationRenderError),
}

/// Renders pages of a [`PdfDocument`] onto a user supplied [`CanvasBackend`].
pub struct PdfRenderer {
    document: PdfDocument,
}

impl PdfRenderer {
    /// Creates a new renderer over the owned PDF `document`.
    ///
    /// The renderer owns the document for its lifetime. Call [`PdfRenderer::render`]
    /// with a mutable canvas backend each time a page should be drawn.
    pub fn new(document: PdfDocument) -> Self {
        Self { document }
    }

    /// Returns the owned document by reference.
    pub fn document(&self) -> &PdfDocument {
        &self.document
    }

    /// Returns the owned document after rendering is complete.
    pub fn into_document(self) -> PdfDocument {
        self.document
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
    pub fn render<B: CanvasBackend>(
        &self,
        canvas_backend: &mut B,
        page_index: usize,
    ) -> Result<(), PdfRendererError> {
        let page = self.page(page_index)?;
        {
            let canvas = PdfCanvas::new(canvas_backend, page, None)?;
            let mut annotation_renderer = AnnotationRenderer::new(canvas);
            if let Some(cs) = &page.contents {
                annotation_renderer.canvas.render_content_stream(
                    cs,
                    None,
                    None,
                    page.resources.as_ref(),
                    None,
                )?;
            }
            if let Some(annotations) = &page.annotations {
                annotation_renderer.render_annotations(annotations)?;
            }
        }
        Ok(())
    }

    /// Renders a PDF page into a [`RecordingCanvas`] for caching.
    ///
    /// This records all drawing commands for a page into a resolution-independent
    /// `RecordingCanvas` that can be replayed to any backend at any size.
    pub fn render_page_to_recording(
        &self,
        page_index: usize,
        width: f32,
        height: f32,
    ) -> Result<RecordingCanvas, PdfRendererError> {
        let page = self.page(page_index)?;
        let mut recording = RecordingCanvas::new(width, height);
        {
            let canvas = PdfCanvas::new(&mut recording, page, None)?;
            let mut annotation_renderer = AnnotationRenderer::new(canvas);
            if let Some(cs) = &page.contents {
                annotation_renderer.canvas.render_content_stream(
                    cs,
                    None,
                    None,
                    page.resources.as_ref(),
                    None,
                )?;
            }
            if let Some(annotations) = &page.annotations {
                annotation_renderer.render_annotations(annotations)?;
            }
        }
        Ok(recording)
    }

    /// Extracts selectable page text for a specific rendered page size.
    pub fn text_layout(
        &self,
        page_index: usize,
        width: f32,
        height: f32,
    ) -> Result<PageTextLayout, PdfRendererError> {
        let page = self.page(page_index)?;
        let mut backend = NoopCanvasBackend { width, height };
        let mut collector = pdf_canvas::text::TextCollector::new();
        {
            let mut canvas =
                PdfCanvas::new_with_text_sink(&mut backend, page, None, &mut collector)?;
            if let Some(cs) = &page.contents {
                canvas.render_content_stream(cs, None, None, page.resources.as_ref(), None)?;
            }
        }

        Ok(PageTextLayout::new(
            collector
                .into_glyphs()
                .into_iter()
                .map(Into::into)
                .collect(),
        ))
    }

    fn page(&self, page_index: usize) -> Result<&pdf_document::page::PdfPage, PdfRendererError> {
        let Some(page) = self.document.pages.get(page_index) else {
            return Err(PdfRendererError::PageNotFound(page_index));
        };
        Ok(page)
    }
}

struct NoopCanvasBackend {
    width: f32,
    height: f32,
}

impl CanvasBackend for NoopCanvasBackend {
    fn fill_path(
        &mut self,
        _path: &PdfPath,
        _fill_type: PathFillType,
        _color: Color,
        _shader: &Option<Shader>,
        _blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        Ok(())
    }

    fn stroke_path(
        &mut self,
        _path: &PdfPath,
        _color: Color,
        _line_width: f32,
        _stroke_style: &StrokeStyle,
        _shader: &Option<Shader>,
        _blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        Ok(())
    }

    fn set_clip_region(
        &mut self,
        _path: &PdfPath,
        _mode: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        Ok(())
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn save(&mut self) -> Result<(), PdfCanvasError> {
        Ok(())
    }

    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        Ok(())
    }

    fn draw_image_rect(
        &mut self,
        _image: &Image<'_>,
        _blend_mode: Option<BlendMode>,
        _dest_rect: Rect,
        _image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        Ok(())
    }

    fn begin_mask_layer(
        &mut self,
        _mask: &std::sync::Arc<RecordingCanvas>,
        _transform: &Transform,
        _mask_mode: MaskMode,
    ) -> Result<(), PdfCanvasError> {
        Ok(())
    }

    fn end_mask_layer(
        &mut self,
        _mask: &std::sync::Arc<RecordingCanvas>,
        _transform: &Transform,
        _mask_mode: MaskMode,
    ) -> Result<(), PdfCanvasError> {
        Ok(())
    }
}

/// Renders a cached [`RecordingCanvas`] to a backend, or renders the page
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
    if let Some(recording) = cache.get(page_index) {
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
