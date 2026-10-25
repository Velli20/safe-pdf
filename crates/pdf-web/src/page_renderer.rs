//! Web orchestration separates existing page rendering from annotation presentation.

use crate::{
    WebViewport,
    annotation_overlay::{PreparedAnnotationOverlay, WebAnnotationOverlay},
    error::{WebError as Error, WebResult},
};
use pdf_document::document::PdfDocument;
use pdf_graphics::size::Size;
use pdf_graphics_web::WebCanvasBackend;
use pdf_renderer::{PageTextLayout, PdfRenderer};
use std::sync::Arc;

/// Results of one page-content pass and independently prepared annotation presentation.
pub struct WebPageOutput {
    /// Existing glyph layout captured with PdfCanvas::with_text_recording.
    pub text_layout: Arc<PageTextLayout>,
    /// Logical device dimensions used to create both glyph and page geometry.
    pub device_size: Size,
    /// Page content revision used by text layout ingestion.
    pub content_revision: u32,
    /// Native annotation presentation, independent of page pixels.
    pub annotations: PreparedAnnotationOverlay,
}

/// Thin orchestration around PdfRenderer; no new content-stream or font rendering engine.
pub struct WebPageRenderer {
    renderer: PdfRenderer,
}

impl WebPageRenderer {
    /// Renders page content of the document `renderer` owns.
    pub fn new(renderer: PdfRenderer) -> Self {
        Self { renderer }
    }

    /// Returns the document whose pages this renderer draws.
    pub fn document(&self) -> &PdfDocument {
        self.renderer.document()
    }

    /// Renders only page content via PdfRenderer, collects its existing TextGlyph output,
    /// and prepares annotations separately through the supplied overlay layer.
    ///
    /// Uses the shared content-and-text pass; annotation preparation produces native
    /// models only. Both passes use the supplied viewport.
    /// Finishes with balanced backend state, retaining no backend borrow in the output.
    /// `page_index` selects both the document page and the Core page in `overlay`.
    /// `content_revision` identifies the text layout independently of annotation edits.
    /// `backend` and `viewport` must describe the same device layout.
    ///
    /// # Errors
    /// Returns backend, PDF interpretation, viewport mismatch, or annotation projection
    /// errors. Failed rendering aborts backend page state without changing Core values.
    pub fn render(
        &self,
        page_index: u32,
        content_revision: u32,
        backend: &mut WebCanvasBackend,
        viewport: &WebViewport,
        overlay: &mut WebAnnotationOverlay,
    ) -> WebResult<WebPageOutput> {
        backend.finish_page()?;
        let device_size = viewport.page().device_size();
        if backend.viewport() != viewport.canvas() {
            return Err(Error::InvalidInput(
                "backend canvas viewport differs from presentation",
            ));
        }
        let result = (|| {
            let index = usize::try_from(page_index).map_err(|_| Error::ResourceLimit)?;
            let text_layout = Arc::new(self.renderer.render_with_text_layout(
                backend,
                index,
                viewport.page(),
                overlay.optional_content(),
            )?);
            backend.finish_page()?;
            let annotations = overlay.prepare(page_index, viewport)?;
            Ok(WebPageOutput {
                text_layout,
                device_size,
                content_revision,
                annotations,
            })
        })();
        if result.is_err() {
            // Keep the original rendering failure if cleanup also fails.
            let _ = backend.abort_page();
        }
        result
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::{WebAnnotationOverlay, WebViewport};
    use pdf_graphics::transform::Transform;
    use pdf_graphics_web::WebCanvasOptions;
    use wasm_bindgen::JsCast;
    fn run(
        bytes: &[u8],
        size: f32,
    ) -> (
        WebPageOutput,
        WebCanvasBackend,
        WebAnnotationOverlay,
        WebPageRenderer,
    ) {
        let document = pdf_document::reader::PdfReader
            .read_from_bytes(bytes, None)
            .unwrap();
        let renderer = WebPageRenderer::new(PdfRenderer::new(document));
        let page = renderer.document().pages.first().unwrap();
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .create_element("canvas")
            .unwrap()
            .dyn_into()
            .unwrap();
        let viewport = WebViewport::new(
            page.viewport(None, Size::new(size, size)).unwrap(),
            Size::new(
                num_traits::ToPrimitive::to_u32(&size).unwrap(),
                num_traits::ToPrimitive::to_u32(&size).unwrap(),
            ),
            Transform::identity(),
            1,
        )
        .unwrap();
        let mut backend =
            WebCanvasBackend::new(canvas, *viewport.canvas(), WebCanvasOptions::default()).unwrap();
        let mut overlay = WebAnnotationOverlay::new(renderer.document()).unwrap();
        let output = renderer
            .render(0, 1, &mut backend, &viewport, &mut overlay)
            .unwrap();
        (output, backend, overlay, renderer)
    }
}
