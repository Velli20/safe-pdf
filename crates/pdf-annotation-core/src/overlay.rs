//! Document-owned Core editing and disposable per-viewport annotation entries.
use crate::{
    commands::Operation,
    engine::DocumentView,
    entry::AnnotationEntry,
    events::{Change, Event},
    import::{self, ImportedDocument, LayoutHint},
    kind::AnnotationKind,
    layer_error::{AnnotationLayerError, AnnotationLayerResult},
    models::{Annotation, AnnotationId, Point, Revision},
    pdf_data::SourceAnnotation,
    projection,
    requests::{AnnotationCommandRequest, AnnotationReceipt, AnnotationTarget},
};
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_object_reader::diagnostic::PdfReadDiagnostic;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Immutable page annotation entries with independent Core and viewport revisions.
///
/// Cloning shares the entries; the overlay hands out the same projection until an
/// edit touches the page or the host viewport changes.
#[derive(Clone)]
pub struct PreparedAnnotations {
    page: u32,
    revision: Revision,
    viewport_revision: u32,
    entries: Arc<[AnnotationEntry]>,
}

impl PreparedAnnotations {
    /// Returns the page containing these entries.
    pub fn page_index(&self) -> u32 {
        self.page
    }

    /// Returns the latest Core revision these entries are known to present.
    ///
    /// Edits to other pages advance this without reprojecting, so it identifies
    /// the document state the entries are valid for, not the projection's origin.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Returns the captured host viewport revision.
    pub fn viewport_revision(&self) -> u32 {
        self.viewport_revision
    }

    /// Borrows the projected entries in presentation order.
    pub fn entries(&self) -> &[AnnotationEntry] {
        &self.entries
    }
}

/// One Core engine per document, independent of page mounts and backend resources.
pub struct AnnotationOverlay {
    document: ImportedDocument,
    pages: BTreeMap<u32, PreparedAnnotations>,
    /// Revision of the last edit touching each annotation; absent means unedited.
    modified: BTreeMap<u64, Revision>,
    /// Revision every imported annotation was last changed at.
    imported: Revision,
}

impl AnnotationOverlay {
    /// Imports one source annotation slice per page once and retains valid
    /// annotations and field groups.
    ///
    /// # Errors
    ///
    /// Returns identity/resource errors on document initialization. Individual bad
    /// source records are retained as diagnostics without partially importing fields.
    pub fn new<'a>(
        pages: impl IntoIterator<Item = &'a [SourceAnnotation]>,
    ) -> AnnotationLayerResult<Self> {
        let document = import::import(pages)?;
        let imported = document.engine.view().data().revision;
        Ok(Self {
            document,
            pages: BTreeMap::new(),
            modified: BTreeMap::new(),
            imported,
        })
    }

    /// Returns import diagnostics ordered by page and object; does not mutate state.
    pub fn diagnostics(&self) -> &[PdfReadDiagnostic] {
        &self.document.diagnostics
    }

    /// Returns the current document revision, independent of mounted pages.
    pub fn revision(&self) -> Revision {
        self.document.engine.view().data().revision
    }

    /// Projects a complete page from borrowed Core state through `page_to_device`, the
    /// host viewport's PDF page to logical device mapping. `viewport_revision`
    /// identifies the host layout so stale events can be rejected.
    ///
    /// # Errors
    ///
    /// Rejects absent pages and invalid geometry. Does not mutate the Core engine or
    /// render page content.
    pub fn prepare(
        &mut self,
        page: u32,
        page_to_device: &Transform,
        viewport_revision: u32,
    ) -> AnnotationLayerResult<PreparedAnnotations> {
        let prepared = self.project_page(page, page_to_device, viewport_revision)?;
        self.pages.insert(page, prepared.clone());
        Ok(prepared)
    }

    /// Returns a cached page or rebuilds its invalidated annotation presentation.
    ///
    /// # Errors
    /// Propagates projection errors. Retrying is safe after a committed edit: this
    /// method never redispatches commands and never reimports deleted PDF records.
    pub fn prepared(
        &mut self,
        page: u32,
        page_to_device: &Transform,
        viewport_revision: u32,
    ) -> AnnotationLayerResult<PreparedAnnotations> {
        match self.cached(page, viewport_revision) {
            Some(cached) => Ok(cached.clone()),
            None => self.prepare(page, page_to_device, viewport_revision),
        }
    }

    /// Validates a GUI context, clamps interactive dragging, and dispatches one edit.
    /// `viewport_revision` and `page_bounds` are required for a mounted target and dragging.
    /// Programmatic operations omit the target and retain Core's unclamped semantics.
    ///
    /// # Errors
    /// Rejects stale revisions, mismatched controls, prohibited edits, invalid bounds,
    /// and Core validation errors before mutation. Successful commands invalidate
    /// caches before returning; later projection errors cannot undo the commit.
    pub fn accept_event(
        &mut self,
        mut command: AnnotationCommandRequest,
        viewport_revision: Option<u32>,
        page_bounds: Option<Rect<f64>>,
    ) -> AnnotationLayerResult<AnnotationReceipt> {
        if let Some(target) = &command.target {
            let annotation = self.mounted_annotation(target, viewport_revision)?;
            authorize(annotation, &command.command.operation)?;
            if let Operation::Translate { delta, .. } = &mut command.command.operation {
                *delta = self.clamp_drag(annotation, *delta, page_bounds)?;
            }
        }
        let deleted_page = self.page_of_deleted(&command.command.operation);
        let replaced = replaced_annotation(&command.command.operation);
        let event = self.document.engine.dispatch(command.command)?;
        let pages = self.record_change(&event, deleted_page);
        if let Some(id) = replaced {
            self.forget_source_rect(id);
        }
        self.invalidate(&pages, event.revision);
        Ok(AnnotationReceipt {
            event,
            pages: pages.into_iter().collect(),
        })
    }

    /// Drops only a page's disposable presentation; Core values survive eviction.
    pub fn remove_page(&mut self, page: u32) {
        self.pages.remove(&page);
    }

    /// Drops all disposable snapshots without deleting annotations or fields.
    pub fn clear(&mut self) {
        self.pages.clear();
    }

    /// The page's snapshot when it still matches both the host viewport and Core.
    fn cached(&self, page: u32, viewport_revision: u32) -> Option<&PreparedAnnotations> {
        self.pages.get(&page).filter(|snapshot| {
            snapshot.viewport_revision == viewport_revision && snapshot.revision == self.revision()
        })
    }

    /// Projects every annotation on `page` without touching the snapshot cache.
    fn project_page(
        &self,
        page: u32,
        page_to_device: &Transform,
        viewport_revision: u32,
    ) -> AnnotationLayerResult<PreparedAnnotations> {
        let view = self.document.engine.view();
        if page >= view.data().document.page_count {
            return Err(AnnotationLayerError::PageOutOfRange(page));
        }
        let annotations = self.page_annotations(&view, page);
        let popup_parents = popup_parents(&annotations);
        let entries = annotations
            .iter()
            .map(|annotation| {
                let has_popup = popup_parents.contains(&annotation.id.0);
                self.project_entry(&view, annotation, page_to_device, has_popup)
            })
            .collect::<AnnotationLayerResult<Arc<[_]>>>()?;
        Ok(PreparedAnnotations {
            page,
            revision: view.data().revision,
            viewport_revision,
            entries,
        })
    }

    /// A page's annotations in source traversal order, with host-created ones last.
    fn page_annotations<'a>(&self, view: &DocumentView<'a>, page: u32) -> Vec<&'a Annotation> {
        let mut annotations = view
            .data()
            .annotations
            .iter()
            .filter(|annotation| annotation.page.0 == page)
            .collect::<Vec<_>>();
        annotations.sort_by_key(|annotation| {
            let order = self
                .document
                .hints
                .get(&annotation.id.0)
                .map_or(usize::MAX, |hint| hint.order);
            (order, annotation.id.0)
        });
        annotations
    }

    /// Projects one annotation with its layout hint and edit revision.
    fn project_entry(
        &self,
        view: &DocumentView<'_>,
        annotation: &Annotation,
        page_to_device: &Transform,
        has_popup: bool,
    ) -> AnnotationLayerResult<AnnotationEntry> {
        projection::project(
            annotation,
            view,
            self.document.hints.get(&annotation.id.0),
            page_to_device,
            self.modified_revision(annotation.id),
            has_popup,
        )
    }

    /// Revision that last changed `id`: its edit, or the import for untouched records.
    fn modified_revision(&self, id: AnnotationId) -> Revision {
        self.modified.get(&id.0).copied().unwrap_or(self.imported)
    }

    /// The annotation a mounted control targets, once its page and viewport are current.
    ///
    /// # Errors
    /// `StaleRevision` when the host viewport differs from the target's or the page is
    /// no longer mounted at that revision; invalid input when the id is not on that page.
    fn mounted_annotation(
        &self,
        target: &AnnotationTarget,
        viewport_revision: Option<u32>,
    ) -> AnnotationLayerResult<&Annotation> {
        let viewport_revision = viewport_revision.ok_or(AnnotationLayerError::StaleRevision)?;
        let mounted = self
            .pages
            .get(&target.page)
            .is_some_and(|snapshot| snapshot.viewport_revision == target.viewport_revision);
        if viewport_revision != target.viewport_revision || !mounted {
            return Err(AnnotationLayerError::StaleRevision);
        }
        self.document
            .engine
            .view()
            .annotation(target.annotation_id)
            .filter(|annotation| annotation.page.0 == target.page)
            .ok_or(AnnotationLayerError::InvalidInput("annotation target"))
    }

    /// Clamps an interactive drag so the annotation stays inside `page_bounds`.
    fn clamp_drag(
        &self,
        annotation: &Annotation,
        delta: Point,
        page_bounds: Option<Rect<f64>>,
    ) -> AnnotationLayerResult<Point> {
        let page = page_bounds.ok_or(AnnotationLayerError::InvalidInput("drag page bounds"))?;
        let layout = self
            .document
            .hints
            .get(&annotation.id.0)
            .and_then(LayoutHint::source_layout);
        let bounds = annotation.content.bounds(layout.as_ref())?;
        bounds
            .clamp_translation(delta, &page)
            .ok_or(AnnotationLayerError::InvalidInput("drag translation"))
    }

    /// Page of a delete target, captured before Core forgets the annotation.
    fn page_of_deleted(&self, operation: &Operation) -> Option<u32> {
        match operation {
            Operation::Delete { id } => self.page_of(*id),
            _ => None,
        }
    }

    fn page_of(&self, id: AnnotationId) -> Option<u32> {
        self.document
            .engine
            .view()
            .annotation(id)
            .map(|annotation| annotation.page.0)
    }

    /// Records which annotations `event` touched and returns the pages to refresh.
    fn record_change(&mut self, event: &Event, deleted_page: Option<u32>) -> BTreeSet<u32> {
        let mut pages = BTreeSet::new();
        match &event.change {
            Change::Created(id) | Change::Updated(id) => {
                pages.extend(self.page_of(*id));
                self.modified.insert(id.0, event.revision);
            }
            Change::Deleted(id) => {
                pages.extend(deleted_page);
                self.document.hints.remove(&id.0);
                self.modified.remove(&id.0);
            }
            Change::FieldUpdated { widgets, .. } => {
                for id in widgets {
                    pages.extend(self.page_of(*id));
                    self.modified.insert(id.0, event.revision);
                }
            }
            _ => {}
        }
        pages
    }

    /// Replaced payloads carry their own geometry; the source rectangle no longer applies.
    fn forget_source_rect(&mut self, id: AnnotationId) {
        if let Some(hint) = self.document.hints.get_mut(&id.0) {
            hint.layout = None;
        }
    }

    /// Drops the touched snapshots and marks the rest valid at `revision`.
    fn invalidate(&mut self, pages: &BTreeSet<u32>, revision: Revision) {
        for page in pages {
            self.pages.remove(page);
        }
        for snapshot in self.pages.values_mut() {
            snapshot.revision = revision;
        }
    }
}

/// Identities named as a parent by a popup among `annotations`.
fn popup_parents(annotations: &[&Annotation]) -> BTreeSet<u64> {
    annotations
        .iter()
        .filter_map(|annotation| match &annotation.content {
            AnnotationKind::Popup(popup) => popup.parent.map(|id| id.0),
            _ => None,
        })
        .collect()
}

/// Whether a mounted control may issue `operation` against its own annotation.
///
/// # Errors
///
/// `TargetMismatch` when the operation names another annotation or field, or the
/// annotation's flags and kind prohibit that edit.
fn authorize(annotation: &Annotation, operation: &Operation) -> AnnotationLayerResult<()> {
    let bound_field = match &annotation.content {
        AnnotationKind::Widget(widget) => Some(widget.binding.field()),
        _ => None,
    };
    let permitted = match operation {
        Operation::Translate { id, .. } => *id == annotation.id && annotation.can_translate(),
        Operation::SetFreeTextText { id, .. } => {
            *id == annotation.id && annotation.can_edit_free_text()
        }
        Operation::SetText { field, .. }
        | Operation::SetCheckbox { field, .. }
        | Operation::SetCheckboxGroupSelection { field, .. }
        | Operation::SetRadioSelection { field, .. }
        | Operation::SetListSelection { field, .. }
        | Operation::SetComboValue { field, .. } => {
            Some(*field) == bound_field && annotation.metadata.can_edit_contents()
        }
        Operation::Delete { id } | Operation::ReplaceContent { id, .. } => *id == annotation.id,
        _ => false,
    };
    permitted
        .then_some(())
        .ok_or(AnnotationLayerError::TargetMismatch)
}

/// Annotation whose payload `operation` replaces wholesale.
fn replaced_annotation(operation: &Operation) -> Option<AnnotationId> {
    match operation {
        Operation::ReplaceContent { id, .. } => Some(*id),
        _ => None,
    }
}
