//! Web orchestration separates existing page rendering from annotation presentation.

use crate::{
    WebViewport,
    annotation_overlay::{PreparedAnnotationOverlay, WebAnnotationOverlay},
    error::{WebError as Error, WebResult},
};
use pdf_document::page::PdfPage;
use pdf_graphics_web::WebCanvasBackend;
use pdf_renderer::PageTextLayout;
use pdf_text_engine::FontSystem;
use std::sync::Arc;

/// Results of one page-content pass and independently prepared annotation presentation.
pub struct WebPageOutput {
    /// Existing glyph layout captured with PdfCanvas::with_text_recording.
    pub text_layout: Arc<PageTextLayout>,
    /// Logical device dimensions used to create both glyph and page geometry.
    pub device_size: [f32; 2],
    /// Page content revision used by text layout ingestion.
    pub content_revision: u32,
    /// Native annotation presentation, independent of page pixels.
    pub annotations: PreparedAnnotationOverlay,
}

/// Thin orchestration around PdfCanvas; no new content-stream or font rendering engine.
pub struct WebPageRenderer {
    fonts: Arc<FontSystem>,
}

impl WebPageRenderer {
    /// Uses the application's shared font system for page content.
    pub fn new(fonts: Arc<FontSystem>) -> Self {
        Self { fonts }
    }

    /// Renders only page content via PdfCanvas, collects its existing TextGlyph output,
    /// and prepares annotations separately through the supplied overlay layer.
    ///
    /// Uses the shared content-and-text pass; annotation preparation produces native
    /// models only. Both passes use the supplied viewport.
    /// Finishes with balanced backend state, retaining no backend borrow in the output.
    /// `page_index` selects the Core page in `overlay`; `page` supplies PDF content.
    /// `content_revision` identifies the text layout independently of annotation edits.
    /// `backend` and `viewport` must describe the same device layout.
    ///
    /// # Errors
    /// Returns backend, PDF interpretation, viewport mismatch, or annotation projection
    /// errors. Failed rendering aborts backend page state without changing Core values.
    pub fn render(
        &self,
        page_index: u32,
        page: &PdfPage,
        content_revision: u32,
        backend: &mut WebCanvasBackend,
        viewport: &WebViewport,
        overlay: &mut WebAnnotationOverlay,
    ) -> WebResult<WebPageOutput> {
        backend.finish_page()?;
        let device_size = viewport.device_size();
        if backend.viewport() != viewport.canvas() {
            return Err(Error::InvalidInput(
                "backend canvas viewport differs from presentation",
            ));
        }
        let result = (|| {
            let text_layout = Arc::new(pdf_renderer::render_page_content_with_text_layout(
                backend,
                page,
                viewport.page(),
                Arc::clone(&self.fonts),
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
        pdf_document::document::PdfDocument,
    ) {
        let document = pdf_document::reader::PdfReader
            .read_from_bytes(bytes, None)
            .unwrap();
        let page = document.pages.first().unwrap();
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .create_element("canvas")
            .unwrap()
            .dyn_into()
            .unwrap();
        let viewport = WebViewport::new(
            pdf_canvas::PageViewport::from_page(page, None, [size, size]).unwrap(),
            [f64::from(size), f64::from(size)],
            [
                num_traits::ToPrimitive::to_u32(&size).unwrap(),
                num_traits::ToPrimitive::to_u32(&size).unwrap(),
            ],
            Transform::identity(),
            1,
        )
        .unwrap();
        let mut backend =
            WebCanvasBackend::new(canvas, *viewport.canvas(), WebCanvasOptions::default()).unwrap();
        let mut overlay = WebAnnotationOverlay::new(&document).unwrap();
        let output = WebPageRenderer::new(pdf_text_engine::bundled_font_system())
            .render(0, page, 1, &mut backend, &viewport, &mut overlay)
            .unwrap();
        (output, backend, overlay, document)
    }
}
