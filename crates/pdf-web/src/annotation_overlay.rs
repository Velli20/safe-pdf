//! Browser adapter around Core's backend-neutral annotation overlay.
use crate::{WebViewport, annotation_entry::WebAnnotationEntry, error::WebResult};
use pdf_annotation_core::{
    ActionReceipt, ActionTrigger, AnnotationCommandRequest, AnnotationOverlay, AnnotationReceipt,
    PaintedAppearance, PreparedAnnotations, models::AnnotationId,
};
use pdf_document::{diagnostic::PdfReadDiagnostic, document::PdfDocument};
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_optional_content::OptionalContentState;

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
    /// The widget held down by the pointer, painted with its down appearance.
    /// Presentation state only; it never advances the document revision.
    pressed: Option<AnnotationId>,
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
            pressed: None,
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
        let prepared = self
            .overlay
            .prepare(page, viewport.page(), viewport.revision())?;
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
        let prepared = self
            .overlay
            .prepared(page, viewport.page(), viewport.revision())?;
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
        page_bounds: Option<Rect>,
    ) -> WebResult<AnnotationReceipt> {
        Ok(self
            .overlay
            .accept_event(command, viewport.map(WebViewport::revision), page_bounds)?)
    }

    /// Runs the action `trigger` selects on annotation `id`: hide steps commit one
    /// Core edit, optional content steps change visibility, and the rest return to
    /// the host.
    ///
    /// # Errors
    /// Rejects an unknown annotation and propagates Core errors from the hide edit.
    pub fn perform_action(
        &mut self,
        id: AnnotationId,
        trigger: ActionTrigger,
    ) -> WebResult<ActionReceipt> {
        Ok(self.overlay.perform_action(id, trigger)?)
    }

    /// Returns the widget appearances painted on `page`'s canvas under live field state,
    /// with the pressed widget's down appearance.
    ///
    /// # Errors
    /// Propagates annotation layer errors for absent pages or unbound widgets.
    pub fn painted_appearances(&self, page: u32) -> WebResult<Vec<PaintedAppearance>> {
        Ok(self.overlay.painted_appearances(page, self.pressed)?)
    }

    /// Marks the widget the pointer holds down, or none; the host repaints its page.
    pub fn set_pressed(&mut self, pressed: Option<AnnotationId>) {
        self.pressed = pressed;
    }

    /// Returns the runtime optional content visibility page content is rendered with.
    pub(crate) fn optional_content(&self) -> &OptionalContentState {
        self.overlay.optional_content()
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
