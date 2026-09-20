//! A small document owner demonstrating the browser crate's host-facing APIs.
use pdf_document::{document::PdfDocument, reader::PdfReader};
use pdf_graphics_web::{WebCanvasBackend, WebCanvasOptions};
use pdf_renderer::DocumentTextSelection;
use pdf_web::{
    WebAnnotationOverlay, WebPageRenderer, WebViewport,
    interop::{
        WebAnnotationEvent, WebAnnotationSnapshot, WebSelectionBatch, WebSelectionController,
    },
};
use std::{collections::BTreeMap, sync::Arc};
use wasm_bindgen::prelude::*;

/// Browser example's document, layouts and overlay resources.
#[wasm_bindgen]
pub struct WebDocument {
    document: PdfDocument,
    renderer: WebPageRenderer,
    overlay: WebAnnotationOverlay,
    selection: WebSelectionController,
    layouts: BTreeMap<u32, u32>,
    viewports: BTreeMap<u32, WebViewport>,
    content_revision: u32,
    viewport_revision: u32,
}

#[wasm_bindgen]
impl WebDocument {
    /// Parses user-provided bytes entirely within WebAssembly.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<WebDocument, JsValue> {
        let document = PdfReader.read_from_bytes(bytes, None).map_err(error)?;
        let count = u32::try_from(document.pages.len()).map_err(error)?;
        let overlay = WebAnnotationOverlay::new(&document).map_err(error)?;
        Ok(Self {
            document,
            renderer: WebPageRenderer::new(pdf_text_engine::bundled_font_system()),
            overlay,
            selection: WebSelectionController::from_selection(
                DocumentTextSelection::new(&(0..count).collect::<Vec<_>>()).map_err(error)?,
            ),
            layouts: BTreeMap::new(),
            viewports: BTreeMap::new(),
            content_revision: 1,
            viewport_revision: 0,
        })
    }

    /// Number of pages in this document.
    pub fn page_count(&self) -> Result<u32, JsValue> {
        u32::try_from(self.document.pages.len()).map_err(error)
    }

    /// Returns the displayed page dimensions in points, honoring CropBox and `/Rotate`.
    pub fn page_size(&self, page: u32) -> Result<Vec<f32>, JsValue> {
        pdf_canvas::PageViewport::page_size(self.page(page)?)
            .map(|size| size.to_vec())
            .map_err(error)
    }

    fn page(&self, page: u32) -> Result<&pdf_document::page::PdfPage, JsValue> {
        page_of(&self.document, page)
    }

    /// Renders page pixels and returns independently prepared annotation models.
    pub fn render(
        &mut self,
        page_index: u32,
        canvas: web_sys::HtmlCanvasElement,
        zoom: f32,
        dpr: f32,
        rotation: u32,
    ) -> Result<WebAnnotationSnapshot, JsValue> {
        self.viewport_revision = self
            .viewport_revision
            .checked_add(1)
            .ok_or_else(|| error("viewport revision overflow"))?;
        let page = page_of(&self.document, page_index)?;
        let viewport = WebViewport::for_page(page, zoom, dpr, rotation, self.viewport_revision)
            .map_err(error)?;
        let size = viewport.device_size();
        let mut backend =
            WebCanvasBackend::new(canvas, *viewport.canvas(), WebCanvasOptions::default())
                .map_err(error)?;
        let output = self
            .renderer
            .render(
                page_index,
                page,
                self.content_revision,
                &mut backend,
                &viewport,
                &mut self.overlay,
            )
            .map_err(error)?;
        if self.layouts.get(&page_index) != Some(&self.content_revision) {
            self.selection
                .install_layout(
                    page_index,
                    self.content_revision,
                    size,
                    Arc::clone(&output.text_layout),
                )
                .map_err(error)?;
            self.layouts.insert(page_index, self.content_revision);
        }
        self.viewports.insert(page_index, viewport);
        Ok(WebAnnotationSnapshot::from_snapshot(output.annotations))
    }

    /// Finds a glyph from container-local CSS coordinates.
    pub fn hit_test(&self, page: u32, x: f32, y: f32) -> Result<Vec<u32>, JsValue> {
        let viewport = self
            .viewports
            .get(&page)
            .ok_or_else(|| error("page is not mounted"))?;
        let p = viewport
            .pointer_to_device(pdf_graphics::point::Point::new(x, y))
            .map_err(error)?;
        self.selection.hit_test(page, p.x, p.y)
    }

    /// Converts a container CSS drag displacement into page units for a mounted page.
    pub fn page_delta(&self, page: u32, dx: f64, dy: f64) -> Result<Vec<f64>, JsValue> {
        self.viewports
            .get(&page)
            .ok_or_else(|| error("page is not mounted"))?
            .css_delta_to_page(dx, dy)
            .map(|delta| delta.to_vec())
            .map_err(error)
    }

    /// Updates the selection from two endpoint triples, or clears it with an empty array.
    pub fn select(&mut self, endpoints: Vec<u32>) -> Result<u32, JsValue> {
        self.selection.select(endpoints)
    }

    /// Returns changed highlight batches for visible pages.
    pub fn selection_updates(
        &self,
        visible: Vec<u32>,
        since: Option<u32>,
    ) -> Result<Vec<WebSelectionBatch>, JsValue> {
        self.selection.updates(visible, since)
    }

    /// Returns selected Unicode text without rendering or extraction.
    pub fn selected_text(&self) -> Result<String, JsValue> {
        self.selection.selected_text()
    }

    /// Returns fresh annotations without rendering page pixels or extracting text.
    pub fn annotations(&mut self, page_index: u32) -> Result<WebAnnotationSnapshot, JsValue> {
        let viewport = self
            .viewports
            .get(&page_index)
            .ok_or_else(|| error("unmounted page"))?;
        let prepared = self.overlay.prepared(page_index, viewport).map_err(error)?;
        Ok(WebAnnotationSnapshot::from_snapshot(prepared))
    }

    /// Commits one validated Core command and returns its event and affected pages as JSON.
    /// Returns a JavaScript error on stale context or rejected edits; accepted edits
    /// remain committed even if a subsequent annotations() projection fails.
    pub fn accept_event(&mut self, event: WebAnnotationEvent) -> Result<String, JsValue> {
        let event = event.into_event();
        let page = event.target.as_ref().map(|t| t.page);
        let viewport = page.and_then(|p| self.viewports.get(&p));
        let bounds = page
            .and_then(|p| usize::try_from(p).ok())
            .and_then(|p| self.document.pages.get(p))
            .and_then(|p| p.crop_box.or(p.media_box))
            .map(|r| {
                let r = r.normalized();
                pdf_graphics::rect::Rect {
                    left: f64::from(r.left),
                    top: f64::from(r.top),
                    right: f64::from(r.right),
                    bottom: f64::from(r.bottom),
                }
            });
        let receipt = self
            .overlay
            .accept_event(event, viewport, bounds)
            .map_err(error)?;
        serde_json::to_string(&receipt).map_err(error)
    }

    /// Dispatches a JSON Core command, including programmatic creation and deletion.
    /// The envelope requires expected_revision; omit target for programmatic edits.
    /// Returns a committed receipt or a JavaScript decoding/validation error.
    pub fn dispatch_command(&mut self, json: &str) -> Result<String, JsValue> {
        self.accept_event(WebAnnotationEvent::new(json)?)
    }

    /// Returns import failures without preventing valid annotations from being edited.
    /// Returns a JavaScript error only if diagnostic serialization fails.
    pub fn annotation_diagnostics(&self) -> Result<String, JsValue> {
        self.overlay.diagnostics_json().map_err(error)
    }

    /// Current decimal Core revision, independent of mounted DOM.
    pub fn annotation_revision(&self) -> String {
        self.overlay.revision()
    }

    /// Evicts a page's browser assets and extracted layout.
    pub fn evict_page(&mut self, page: u32) -> Result<(), JsValue> {
        self.overlay.remove_page(page);
        self.selection.evict_page(page)?;
        self.layouts.remove(&page);
        self.viewports.remove(&page);
        Ok(())
    }
}

/// Borrows one page so rendering can hold it alongside mutable overlay state.
fn page_of(document: &PdfDocument, page: u32) -> Result<&pdf_document::page::PdfPage, JsValue> {
    document
        .pages
        .get(usize::try_from(page).map_err(error)?)
        .ok_or_else(|| error("unknown page"))
}

/// Converts a rendering diagnostic into the JavaScript exception boundary.
fn error(value: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&value.to_string())
}
