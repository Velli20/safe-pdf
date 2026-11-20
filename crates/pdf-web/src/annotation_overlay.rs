//! Browser adapter around Core's backend-neutral annotation overlay.
use crate::{WebViewport, annotation_entry::WebAnnotationEntry, error::WebResult};
use pdf_annotation_core::{
    ActionReceipt, ActionTrigger, AnnotationCommandRequest, AnnotationOverlay, AnnotationReceipt,
    PaintedAppearance, PreparedAnnotations, PresentationState, models::AnnotationId,
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
    /// The pressed widget and the edited text entry the page canvas is painted with.
    presentation: PresentationState,
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
            overlay: AnnotationOverlay::new(
                pages,
                document.optional_content.as_ref(),
                document
                    .interactive_form
                    .as_ref()
                    .and_then(|form| form.default_appearance.as_ref()),
            )?,
            presentation: PresentationState::default(),
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

    /// Returns the appearances of `page`'s annotations drawn from their appearance, painted
    /// on the page canvas.
    ///
    /// # Errors
    /// Propagates annotation layer errors for absent pages.
    pub fn painted_appearances(&self, page: u32) -> WebResult<Vec<PaintedAppearance>> {
        Ok(self.overlay.painted_appearances(page)?)
    }

    /// Returns the source artwork of the displayed stamp, note, free text or widget `id`,
    /// drawn into the annotation's own presentation, with the pressed widget's down
    /// appearance and the edited text entry without its value; `None` for other
    /// annotations.
    ///
    /// # Errors
    /// Rejects a widget whose bound field is missing.
    pub fn artwork(&self, id: AnnotationId) -> WebResult<Option<PaintedAppearance>> {
        Ok(self.overlay.artwork(id, self.presentation)?)
    }

    /// Returns the painted appearance of the displayed text entry `id` with its live value;
    /// `None` for other annotations.
    ///
    /// # Errors
    /// Rejects a widget whose bound field is missing.
    pub fn text_entry(&self, id: AnnotationId) -> WebResult<Option<PaintedAppearance>> {
        Ok(self.overlay.text_entry(id)?)
    }

    /// Marks the widget the pointer holds down, or none; the host redraws its artwork.
    pub fn set_pressed(&mut self, pressed: Option<AnnotationId>) {
        self.presentation.pressed = pressed;
    }

    /// Marks the text entry the host is editing, or none; the host redraws its artwork so
    /// only the control shows the value being edited.
    pub fn set_editing(&mut self, editing: Option<AnnotationId>) {
        self.presentation.editing = editing;
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
