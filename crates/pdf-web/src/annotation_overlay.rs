//! Browser adapter around Core's backend-neutral annotation overlay.
use crate::{WebViewport, annotation_entry::WebAnnotationEntry, error::WebResult};
use pdf_annotation_core::{
    AnnotationCommandRequest, AnnotationOverlay, AnnotationReceipt, OptionalContentReceipt,
    PreparedAnnotations, pdf_data::AnnotationAction,
};
use pdf_document::{diagnostic::PdfReadDiagnostic, document::PdfDocument};
use pdf_graphics::{rect::Rect, transform::Transform};

/// Prepared page entries placed for the DOM host.
#[derive(Clone)]
pub struct PreparedAnnotationOverlay {
    prepared: PreparedAnnotations,
    entries: Vec<WebAnnotationEntry>,
    device_to_css: Transform,
}

impl PreparedAnnotationOverlay {
    fn new(prepared: PreparedAnnotations, viewport: &WebViewport) -> Self {
        let device_to_css = *viewport.device_to_css();
        let entries = prepared
            .entries()
            .iter()
            .map(|entry| WebAnnotationEntry::new(entry.clone(), &device_to_css))
            .collect();
        Self {
            prepared,
            entries,
            device_to_css,
        }
    }

    /// Returns the page containing these entries.
    pub fn page_index(&self) -> u32 {
        self.prepared.page_index()
    }

    /// Returns the lossless decimal Core revision these entries are valid for.
    pub fn content_revision(&self) -> String {
        self.prepared.revision().0.to_string()
    }

    /// Returns the captured host viewport revision.
    pub fn viewport_revision(&self) -> u32 {
        self.prepared.viewport_revision()
    }

    /// Borrows the placed entries.
    pub fn entries(&self) -> &[WebAnnotationEntry] {
        &self.entries
    }

    /// Serializes current entries; returns a contextual JSON encoding error on failure.
    pub fn entries_json(&self) -> WebResult<String> {
        Ok(serde_json::to_string(self.entries())?)
    }

    /// Borrows the logical device-to-CSS transform.
    pub fn device_to_css(&self) -> &Transform {
        &self.device_to_css
    }
}

/// One Core engine per document, independent of page mounts and canvas resources.
pub struct WebAnnotationOverlay {
    overlay: AnnotationOverlay,
}

impl WebAnnotationOverlay {
    /// Imports `document` once and retains valid annotations and field groups.
    ///
    /// # Errors
    /// Returns identity/resource errors on document initialization.
    pub fn new(document: &PdfDocument) -> WebResult<Self> {
        let pages = document
            .pages
            .iter()
            .map(|page| page.annotations.as_deref().unwrap_or_default());
        Ok(Self {
            overlay: AnnotationOverlay::new(pages, document.optional_content.as_ref())?,
        })
    }

    /// Returns import diagnostics ordered by page and object; does not mutate state.
    pub fn diagnostics(&self) -> &[PdfReadDiagnostic] {
        self.overlay.diagnostics()
    }

    /// Serializes source import diagnostics, returning JSON errors without panicking.
    pub fn diagnostics_json(&self) -> WebResult<String> {
        Ok(serde_json::to_string(self.diagnostics())?)
    }

    /// Returns the current lossless decimal document revision, independent of mounted pages.
    pub fn revision(&self) -> String {
        self.overlay.revision().0.to_string()
    }

    /// Projects a complete page into the supplied browser viewport.
    ///
    /// # Errors
    /// Propagates annotation layer projection errors.
    pub fn prepare(
        &mut self,
        page: u32,
        viewport: &WebViewport,
    ) -> WebResult<PreparedAnnotationOverlay> {
        let prepared =
            self.overlay
                .prepare(page, viewport.page().page_to_device(), viewport.revision())?;
        Ok(PreparedAnnotationOverlay::new(prepared, viewport))
    }

    /// Returns a cached page or rebuilds its invalidated entries.
    ///
    /// # Errors
    /// Propagates annotation layer projection errors.
    pub fn prepared(
        &mut self,
        page: u32,
        viewport: &WebViewport,
    ) -> WebResult<PreparedAnnotationOverlay> {
        let prepared =
            self.overlay
                .prepared(page, viewport.page().page_to_device(), viewport.revision())?;
        Ok(PreparedAnnotationOverlay::new(prepared, viewport))
    }

    /// Validates a browser context, clamps interactive dragging, and dispatches one edit.
    ///
    /// # Errors
    /// Propagates annotation layer validation and Core errors.
    pub fn accept_event(
        &mut self,
        command: AnnotationCommandRequest,
        viewport: Option<&WebViewport>,
        page_bounds: Option<Rect<f64>>,
    ) -> WebResult<AnnotationReceipt> {
        Ok(self
            .overlay
            .accept_event(command, viewport.map(WebViewport::revision), page_bounds)?)
    }

    /// Applies a `SetOCGState` action to runtime optional content visibility.
    ///
    /// # Errors
    /// Returns an error when the action is not a `SetOCGState` action.
    pub fn set_optional_content_state(
        &mut self,
        action: &AnnotationAction,
    ) -> WebResult<OptionalContentReceipt> {
        Ok(self.overlay.set_optional_content_state(action)?)
    }

    /// Drops only a page's disposable entries; Core values survive eviction.
    pub fn remove_page(&mut self, page: u32) {
        self.overlay.remove_page(page);
    }

    /// Drops all disposable snapshots without deleting annotations or fields.
    pub fn clear(&mut self) {
        self.overlay.clear();
    }
}
