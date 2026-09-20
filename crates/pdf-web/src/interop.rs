//! Concrete WASM exports; generic engine traits and borrowed PDF inputs stay in Rust.

use crate::{
    annotation_overlay::PreparedAnnotationOverlay,
    error::{WebError, WebResult},
};
use pdf_annotation_core::AnnotationCommandRequest;
use pdf_renderer::{
    DocumentTextSelection, PageTextLayout, SelectionBatch, SelectionPoint, SelectionSpan,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// Owned JS-facing annotation snapshot; host components own DOM, events, and object URLs.
#[wasm_bindgen]
pub struct WebAnnotationSnapshot {
    snapshot: PreparedAnnotationOverlay,
}

impl WebAnnotationSnapshot {
    /// Converts prepared Rust output into the concrete browser export wrapper.
    pub fn from_snapshot(snapshot: PreparedAnnotationOverlay) -> Self {
        Self { snapshot }
    }
}

#[wasm_bindgen]
impl WebAnnotationSnapshot {
    /// Returns the owning document page index.
    pub fn page(&self) -> u32 {
        self.snapshot.page_index()
    }

    /// Orders complete snapshots, including annotation removals and visibility changes.
    pub fn content_revision(&self) -> String {
        self.snapshot.content_revision()
    }

    /// Allows the host to discard snapshots with obsolete positioning.
    pub fn viewport_revision(&self) -> u32 {
        self.snapshot.viewport_revision()
    }

    /// Exports one JSON array of `WebAnnotationEntry` values. Each entry carries its
    /// identity, subtype, decoded text, device bounds, typed native presentation,
    /// control model, local shapes, and ready-to-assign CSS. IDs use lossless
    /// decimal strings; text is host text content, never trusted HTML.
    pub fn entries_json(&self) -> Result<String, JsValue> {
        self.snapshot.entries_json().map_err(js_error)
    }

    /// Returns the device-to-CSS matrix as `[a,b,c,d,e,f]` for host placement.
    pub fn device_to_css(&self) -> Vec<f64> {
        matrix(self.snapshot.device_to_css())
    }
}

/// A changed page's custom-selection highlights, separate from the content canvas.
#[wasm_bindgen]
pub struct WebSelectionBatch {
    batch: SelectionBatch,
}

impl WebSelectionBatch {
    /// Wraps geometry produced from the existing renderer text layout.
    pub fn from_batch(batch: SelectionBatch) -> Self {
        Self { batch }
    }
}

#[wasm_bindgen]
impl WebSelectionBatch {
    /// Returns the affected document page.
    pub fn page(&self) -> u32 {
        self.batch.page()
    }

    /// Returns the extraction revision to reject obsolete geometry.
    pub fn layout_revision(&self) -> u32 {
        self.batch.layout_revision()
    }

    /// Returns the selection revision to reject delayed pointer updates.
    pub fn selection_revision(&self) -> u32 {
        self.batch.selection_revision()
    }

    /// Returns the retained layout's logical device dimensions for viewport mapping.
    pub fn device_size(&self) -> Vec<f32> {
        self.batch.device_size().to_vec()
    }

    /// Returns stable highlight IDs, one per exported rectangle, for host element reuse.
    pub fn keys(&self) -> Vec<u32> {
        self.batch.keys().to_vec()
    }

    /// Returns packed `[left,top,right,bottom]` in the cached layout's device space.
    /// Empty batches clear old highlights. Host CSS transforms retain display rotation;
    /// bounds preserve current TextGlyph accuracy rather than inventing glyph outlines.
    /// The host ignores pointer events on highlights and batches writes once per frame.
    pub fn bounds(&self) -> Vec<f32> {
        self.batch
            .bounds()
            .iter()
            .flat_map(|r| [r.left, r.top, r.right, r.bottom])
            .collect()
    }
}

/// Main-thread selection facade; the Rust document layer supplies PageTextLayout instances.
#[wasm_bindgen]
pub struct WebSelectionController {
    selection: DocumentTextSelection,
}

impl WebSelectionController {
    /// Exposes a Rust-populated cache to the host framework.
    pub fn from_selection(selection: DocumentTextSelection) -> Self {
        Self { selection }
    }

    /// Installs existing extracted text as a page becomes available; no duplicate text model.
    pub fn install_layout(
        &mut self,
        page: u32,
        revision: u32,
        size: [f32; 2],
        layout: Arc<PageTextLayout>,
    ) -> WebResult<()> {
        self.selection
            .install_layout(page, revision, size, layout)
            .map_err(WebError::from)
    }
}

#[wasm_bindgen]
impl WebSelectionController {
    /// Returns `[page,layout_revision,glyph_index]`, or an empty array when there is no text.
    /// Coordinates must be mapped into the retained layout's device space by the host.
    pub fn hit_test(&self, page: u32, x: f32, y: f32) -> Result<Vec<u32>, JsValue> {
        self.selection
            .hit_test(page, pdf_graphics::point::Point::new(x, y))
            .map_err(WebError::from)
            .and_then(|hit| match hit {
                Some(hit) => Ok(vec![
                    hit.page,
                    hit.layout_revision,
                    u32::try_from(hit.glyph_index)
                        .map_err(|_| crate::error::WebError::ResourceLimit)?,
                ]),
                None => Ok(Vec::new()),
            })
            .map_err(js_error)
    }

    /// Sets anchor/focus triples or clears with empty input; validates lengths and revisions.
    pub fn select(&mut self, endpoints: Vec<u32>) -> Result<u32, JsValue> {
        let point = |page, layout_revision, index| -> Result<SelectionPoint, JsValue> {
            Ok(SelectionPoint {
                page,
                layout_revision,
                glyph_index: usize::try_from(index)
                    .map_err(|_| js_error(crate::error::WebError::ResourceLimit))?,
            })
        };
        let span = match endpoints.as_slice() {
            [] => None,
            [a, ar, ai, b, br, bi] => Some(SelectionSpan {
                anchor: point(*a, *ar, *ai)?,
                focus: point(*b, *br, *bi)?,
            }),
            _ => {
                return Err(js_error(crate::error::WebError::InvalidInput(
                    "selection endpoint length",
                )));
            }
        };
        self.selection
            .select(span)
            .map_err(WebError::from)
            .map_err(js_error)
    }

    /// Returns one batch per changed visible page; None requests a full visible snapshot.
    pub fn updates(
        &self,
        visible: Vec<u32>,
        since: Option<u32>,
    ) -> Result<Vec<WebSelectionBatch>, JsValue> {
        self.selection
            .updates(&visible, since)
            .map_err(WebError::from)
            .map(|batches| {
                batches
                    .into_iter()
                    .map(WebSelectionBatch::from_batch)
                    .collect()
            })
            .map_err(js_error)
    }

    /// Returns Unicode using the existing renderer's selection/copy behavior.
    pub fn selected_text(&self) -> Result<String, JsValue> {
        self.selection
            .selected_text()
            .map_err(WebError::from)
            .map_err(js_error)
    }

    /// Releases one cache entry; dependent selections become explicitly stale.
    pub fn evict_page(&mut self, page: u32) -> Result<(), JsValue> {
        self.selection
            .evict_page(page)
            .map_err(WebError::from)
            .map_err(js_error)
    }
}

/// Checked browser command decoded before entering the document engine.
#[wasm_bindgen]
pub struct WebAnnotationEvent {
    event: AnnotationCommandRequest,
}
#[wasm_bindgen]
impl WebAnnotationEvent {
    /// Decodes a command envelope with decimal 64-bit identities and revisions.
    /// Returns a JavaScript error for invalid JSON or invalid decimal values.
    #[wasm_bindgen(constructor)]
    pub fn new(json: &str) -> Result<WebAnnotationEvent, JsValue> {
        Ok(Self {
            event: serde_json::from_str(json)
                .map_err(WebError::from)
                .map_err(js_error)?,
        })
    }
}

impl WebAnnotationEvent {
    /// Transfers the owned envelope for host-context validation and Core dispatch.
    pub fn into_event(self) -> AnnotationCommandRequest {
        self.event
    }
}

/// Converts an affine transform into the six values consumed by browser APIs.
fn matrix(t: &pdf_graphics::transform::Transform) -> Vec<f64> {
    [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty]
        .into_iter()
        .map(f64::from)
        .collect()
}

/// Preserves browser exceptions and stringifies other web backend errors.
fn js_error(error: crate::error::WebError) -> JsValue {
    match error {
        crate::error::WebError::Backend(pdf_graphics_web::WebCanvasBackendError::Browser(
            value,
        )) => value,
        other => JsValue::from_str(&other.to_string()),
    }
}
