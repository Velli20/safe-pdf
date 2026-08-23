use pdf_annotations::{AnnotationRenderError, AnnotationRenderer};
use pdf_canvas::{
    canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
    recording_canvas::RecordingCanvas,
};
use pdf_document::document::PdfDocument;
use thiserror::Error;

pub mod page_cache;
pub mod text_selection;

pub use page_cache::PageRecordingCache;
pub use text_selection::{PageTextLayout, TextGlyph};

/// A page's drawing commands and selectable text captured in one render pass.
#[derive(Clone)]
pub struct RecordedPage {
    recording: RecordingCanvas,
    text_layout: PageTextLayout,
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

    /// Replays this page onto a concrete backend.
    pub fn replay<B: CanvasBackend>(&self, backend: &mut B) -> Result<(), PdfCanvasError> {
        self.recording.replay(backend)
    }

    /// Consumes this page and returns its drawing commands and text layout.
    pub fn into_parts(self) -> (RecordingCanvas, PageTextLayout) {
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

    /// Returns the owned document mutably for in-memory editing.
    pub fn document_mut(&mut self) -> &mut PdfDocument {
        &mut self.document
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
                annotation_renderer.canvas_mut().render_content_stream(
                    cs,
                    None,
                    None,
                    page.resources.as_deref(),
                    None,
                )?;
            }
            if let Some(annotations) = &page.annotations {
                annotation_renderer.render_all(annotations)?;
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
        let canvas = PdfCanvas::new(canvas_backend, page, None)?.with_text_recording();
        let mut annotation_renderer = AnnotationRenderer::new(canvas);
        if let Some(cs) = &page.contents {
            annotation_renderer.canvas_mut().render_content_stream(
                cs,
                None,
                None,
                page.resources.as_deref(),
                None,
            )?;
        }
        let glyphs = annotation_renderer.canvas_mut().take_text_glyphs();
        if let Some(annotations) = &page.annotations {
            annotation_renderer.render_all(annotations)?;
        }
        Ok(PageTextLayout::new(glyphs))
    }

    /// Renders a PDF page into combined drawing and text recordings for caching.
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
            let canvas = PdfCanvas::new(&mut recording, page, None)?.with_text_recording();
            let mut annotation_renderer = AnnotationRenderer::new(canvas);
            if let Some(cs) = &page.contents {
                annotation_renderer.canvas_mut().render_content_stream(
                    cs,
                    None,
                    None,
                    page.resources.as_deref(),
                    None,
                )?;
            }
            let glyphs = annotation_renderer.canvas_mut().take_text_glyphs();
            if let Some(annotations) = &page.annotations {
                annotation_renderer.render_all(annotations)?;
            }
            glyphs
        };
        Ok(RecordedPage {
            recording,
            text_layout: PageTextLayout::new(glyphs),
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

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::expect_used)]
mod tests {
    use pdf_document::reader::PdfReader;

    use super::PdfRenderer;

    fn append_stream_object(data: &mut Vec<u8>, number: usize, dictionary: &str, content: &[u8]) {
        data.extend_from_slice(
            format!(
                "{number} 0 obj\n<< {dictionary} /Length {} >>\nstream\n",
                content.len().saturating_add(1)
            )
            .as_bytes(),
        );
        data.extend_from_slice(content);
        data.extend_from_slice(b"\nendstream\nendobj\n");
    }

    fn xref_less_recursive_form_pdf() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.4\n");
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
        data.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 500 300] \
              /Contents 4 0 R /Resources << /XObject << /X1 6 0 R >> >> >>\nendobj\n",
        );
        append_stream_object(&mut data, 4, "", b"1 0 0 1 25 25 cm /X1 Do");
        append_stream_object(
            &mut data,
            6,
            "/Subtype /Form /BBox [0 0 1000 1000] \
             /Resources << /Font << /F1 7 0 R >> /XObject << /X0 8 0 R >> >>",
            b"BT /F1 24 Tf (Hello world) Tj ET 0.5 0 0 0.5 25 25 cm /X0 Do",
        );
        data.extend_from_slice(
            b"7 0 obj\n<< /Type /Font /Subtype /Type1 /Name /F1 \
              /BaseFont /Helvetica /Encoding /MacRomanEncoding >>\nendobj\n",
        );
        append_stream_object(
            &mut data,
            8,
            "/Subtype /Form /BBox [0 0 1000 1000] \
             /Resources << /Font << /F1 7 0 R >> /XObject << /X1 6 0 R >> >>",
            b"BT /F1 24 Tf (Hello world) Tj ET 0.5 0 0 0.5 25 25 cm /X1 Do",
        );
        data.extend_from_slice(b"trailer\n<< /Size 9 /Root 1 0 R >>\n%%EOF\n");
        data
    }

    fn text_after_stream_length_recovery_pdf() -> &'static [u8] {
        b"\n\n%PDF-1.1\n\
1 0 obj\n<</Type/Catalog/Pages 2 0 R>>\nendobj\n\
2 0 obj\n<</Type/Pages/Count 1/Kids[3 0 R]/MediaBox [0 0 400 50]>>\nendobj\n\
3 0 obj\n<</Type/Page/Parent 2 0 R/Resources<</Font<</F1<</Type/Font/Subtype/Type1/BaseFont/Arial>>>>>>/Contents 4 0 R>>\nendobj\n\
4 0 obj\n<</Length 43>>\nstream\n\
BT/F1 14 Tf 20 20 Td(TEST) Tj ET\n\
endstream\nendobj\n\
xref\n0000000000 65535 f \n0000000008 00000 n \n0000000054 00000 n \n0000000128 00000 n \n0000000254 00000 n \n\
trailer\n<</Root 1 0 R/Size 5>>\nstartxref\n"
    }

    #[test]
    fn renders_text_after_stream_length_recovery() {
        let document = PdfReader
            .read_from_bytes(text_after_stream_length_recovery_pdf(), None)
            .expect("PDF should load through xref reconstruction");

        let recorded = PdfRenderer::new(document)
            .render_page_to_recording(0, 400.0, 50.0)
            .expect("PDF should render its recovered content stream");

        assert_eq!(recorded.text_layout().glyphs().len(), 4);
    }

    #[test]
    fn renders_xref_less_pdf_with_mutually_recursive_forms() {
        let data = xref_less_recursive_form_pdf();
        let document = PdfReader
            .read_from_bytes(&data, None)
            .expect("xref reconstruction should load the PDF");
        assert_eq!(document.page_count(), 1);

        let recorded = PdfRenderer::new(document)
            .render_page_to_recording(0, 500.0, 300.0)
            .expect("recursive form rendering should stop at the depth limit");
        let glyphs = recorded.text_layout().glyphs();

        assert!(glyphs.len() >= 11 * 3);
        assert_eq!(glyphs.len() % 11, 0);
        let first_width = glyphs
            .first()
            .expect("first repeated glyph should exist")
            .bounds
            .width()
            .abs();
        let second_width = glyphs
            .get(11)
            .expect("second repeated glyph should exist")
            .bounds
            .width()
            .abs();
        let third_width = glyphs
            .get(22)
            .expect("third repeated glyph should exist")
            .bounds
            .width()
            .abs();

        assert!((second_width / first_width - 0.5).abs() < 0.01);
        assert!((third_width / second_width - 0.5).abs() < 0.01);
    }
}
