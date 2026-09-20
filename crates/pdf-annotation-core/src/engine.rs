use std::marker::PhantomData;

use crate::commands::{AnnotationCommand, Operation};
use crate::error::{CoreError, ValidationError};
use crate::events::{Change, Event};
use crate::fields::{Field, FieldId, FieldIssue, FieldIssueKind, FieldKind};
use crate::kind::AnnotationKind;
use crate::models::{
    Annotation, AnnotationId, DocumentData, DocumentDescriptor, PageIndex, Revision, SCHEMA_VERSION,
};
use crate::store::AnnotationStore;

/// Marker for a snapshot with no storage acknowledgement.
#[derive(Debug)]
pub enum Unsaved {}

/// Marker proving storage acknowledged this exact snapshot, not future edits.
#[derive(Debug)]
pub enum Persisted {}

/// Immutable owned snapshot whose persistence state cannot be forged via serde.
///
/// Serialize its DTO through [`Self::data`]. Only [`AnnotationEngine::save`]
/// creates a persisted snapshot. Neither marker is stored in sidecar JSON.
///
/// ```compile_fail
/// use pdf_annotation_core::engine::{Persisted, Snapshot};
/// use pdf_annotation_core::models::DocumentData;
/// fn forge(data: DocumentData) -> Snapshot<Persisted> {
///     Snapshot { data, state: std::marker::PhantomData }
/// }
/// ```
///
/// Deserialization yields data, never a storage acknowledgement:
///
/// ```compile_fail
/// use pdf_annotation_core::engine::{Persisted, Snapshot};
/// fn decode(json: &str) -> Result<Snapshot<Persisted>, serde_json::Error> {
///     serde_json::from_str(json)
/// }
/// ```
#[derive(Debug)]
pub struct Snapshot<State> {
    data: DocumentData,
    state: PhantomData<State>,
}

impl<State> Snapshot<State> {
    /// Borrow snapshot data for serialization or inspection without mutation.
    pub fn data(&self) -> &DocumentData {
        &self.data
    }
}

/// Immutable, allocation-free access to live state, scoped to an engine borrow.
///
/// An outstanding view prevents mutable command dispatch or saving.
///
/// ```compile_fail
/// use pdf_annotation_core::engine::DocumentView;
/// fn mutate(view: DocumentView<'_>) {
///     view.data().annotations.clear();
/// }
/// ```
pub struct DocumentView<'a> {
    data: &'a DocumentData,
    last_saved_revision: Option<Revision>,
}

impl<'a> DocumentView<'a> {
    /// Borrow the validated document DTO for host rendering or serialization.
    pub fn data(&self) -> &'a DocumentData {
        self.data
    }

    /// Borrow an annotation by identity, returning `None` when it is absent.
    pub fn annotation(&self, id: AnnotationId) -> Option<&'a Annotation> {
        self.data
            .annotations
            .iter()
            .find(|annotation| annotation.id == id)
    }

    /// Borrow a shared field without exposing mutable state to the host.
    pub fn field(&self, id: FieldId) -> Option<&'a Field> {
        self.data.fields.iter().find(|field| field.id == id)
    }

    /// Report missing required values in field insertion order for host submission.
    ///
    /// Empty text, unchecked checkboxes, unselected radio/list/combo fields, and
    /// empty combo custom text are incomplete. Whitespace text and selected options
    /// with empty export values count as filled. Include read-only and unplaced
    /// fields. Return at most one issue per field without changing any revision.
    /// Incompleteness never prevents creation, loading, value edits, or saving.
    pub fn completeness_issues(&self) -> Vec<FieldIssue> {
        self.data
            .fields
            .iter()
            .filter(|field| field.definition.required && field.definition.content.is_empty())
            .map(|field| FieldIssue {
                field: field.id,
                kind: FieldIssueKind::RequiredValueMissing,
            })
            .collect()
    }

    /// Return the last successfully saved revision, or `None` before first save.
    pub fn last_saved_revision(&self) -> Option<Revision> {
        self.last_saved_revision
    }

    /// Report whether the current revision lacks storage acknowledgement.
    pub fn is_dirty(&self) -> bool {
        self.last_saved_revision != Some(self.data.revision)
    }
}

/// Exclusive owner of validated live state and its host-provided store.
///
/// Commands apply only in memory. Each accepted command advances the revision
/// once, including equivalent-content edits; rejected commands change nothing.
/// Loading and commands validate all data before replacing live state. Geometry
/// need not fit page bounds because this boundary knows page count, not size.
///
/// The store contract requires hosts to coordinate concurrent writers. This
/// synchronous API introduces neither a GUI dependency nor a threading runtime.
pub struct AnnotationEngine<S: AnnotationStore> {
    store: S,
    data: DocumentData,
    last_saved_revision: Option<Revision>,
}

impl<S: AnnotationStore> AnnotationEngine<S> {
    /// Create an empty schema-six document at revision zero with both next IDs one.
    ///
    /// Reject an empty document identity. Creation performs no storage I/O;
    /// zero-page documents are allowed but cannot receive annotations.
    /// `document` supplies immutable host identity and page count; `store` is
    /// retained for explicit load/save calls. This function does not panic.
    ///
    /// # Errors
    /// Returns [`ValidationError::EmptyDocumentId`] for an empty identity.
    pub fn new(document: DocumentDescriptor, store: S) -> Result<Self, CoreError<S::Error>> {
        if document.id.0.is_empty() {
            return Err(ValidationError::EmptyDocumentId.into());
        }
        Ok(Self {
            store,
            data: DocumentData {
                schema_version: SCHEMA_VERSION,
                document,
                revision: Revision(0),
                next_annotation_id: 1,
                next_field_id: 1,
                fields: Vec::new(),
                annotations: Vec::new(),
            },
            last_saved_revision: None,
        })
    }

    /// Load a complete sidecar and validate it before replacing live state.
    ///
    /// Check schema six, exact document identity and page count, unique annotation
    /// and field IDs, both allocation counters, page indices, all payloads, and
    /// widget bindings. Reject older schemas; conversion is future migration work.
    /// Incomplete required fields and initially unselected radios are valid.
    /// Success marks the
    /// loaded revision saved. Missing, malformed, or invalid data leaves all
    /// live state unchanged. This explicit operation discards pending edits only
    /// on success; the host decides when requesting such a reload is appropriate.
    ///
    /// # Errors
    /// Returns storage, missing-sidecar, serialization, schema, descriptor, or
    /// validation errors (including missing referenced fields). It does not panic.
    pub fn load(&mut self) -> Result<(), CoreError<S::Error>> {
        let bytes = self
            .store
            .load(&self.data.document.id)
            .map_err(CoreError::Storage)?
            .ok_or(CoreError::SidecarMissing)?;
        // Read the version before schema-specific fields: older schemas have different fields.
        #[derive(serde::Deserialize)]
        struct Version {
            schema_version: u32,
        }
        let version: Version = serde_json::from_slice(&bytes)?;
        if version.schema_version != SCHEMA_VERSION {
            return Err(CoreError::UnsupportedSchema(version.schema_version));
        }
        let data: DocumentData = serde_json::from_slice(&bytes)?;
        if data.document != self.data.document {
            return Err(CoreError::DocumentMismatch {
                expected: self.data.document.clone(),
                actual: data.document,
            });
        }
        Self::validate_document(&data)?;
        self.last_saved_revision = Some(data.revision);
        self.data = data;
        Ok(())
    }

    /// Atomically validate and apply a semantic command, returning its event.
    ///
    /// Reject stale revisions, absent targets, kind changes, invalid resulting
    /// geometry, and counter overflow. Commands never access storage. Core does
    /// not clamp coordinates; translation must leave every result finite.
    ///
    /// Field value commands additionally check kind, read-only status, selection
    /// constraints, and text limits. Configuration is immutable after creation.
    /// Widget replacement changes bounds only, preserving its binding and style;
    /// deleting a widget preserves its field. Deleting a referenced field is rejected. Every accepted field value
    /// command emits all referencing widget IDs, even for equivalent values.
    /// `command` contains the caller's expected revision and complete semantic edit.
    /// The returned event identifies the committed change and its new revision.
    ///
    /// # Errors
    /// Returns the corresponding [`CoreError`] for revision conflicts, missing
    /// targets, prohibited edits, invalid data, or exhausted counters. FreeText
    /// edits reject rich/callout/effect variants; live flags constrain edits and
    /// deletion. Popup references prevent deleting their parent. It does not
    /// panic and performs no storage I/O.
    pub fn dispatch(&mut self, command: AnnotationCommand) -> Result<Event, CoreError<S::Error>> {
        if command.expected_revision != self.data.revision {
            return Err(CoreError::RevisionConflict {
                expected: command.expected_revision,
                actual: self.data.revision,
            });
        }
        let revision = Revision(
            self.data
                .revision
                .0
                .checked_add(1)
                .ok_or(CoreError::CounterExhausted)?,
        );
        let change = match command.operation {
            Operation::Create { annotation } => {
                annotation.metadata.validate()?;
                Self::validate_annotation(&self.data, annotation.page, &annotation.content)?;
                let next = self
                    .data
                    .next_annotation_id
                    .checked_add(1)
                    .ok_or(CoreError::CounterExhausted)?;
                let id = AnnotationId(self.data.next_annotation_id);
                self.data.annotations.push(Annotation {
                    metadata: annotation.metadata,
                    id,
                    page: annotation.page,
                    content: annotation.content,
                });
                self.data.next_annotation_id = next;
                Change::Created(id)
            }
            Operation::SetFreeTextText { id, text } => {
                let annotation = self
                    .data
                    .annotations
                    .iter_mut()
                    .find(|a| a.id == id)
                    .ok_or(CoreError::MissingAnnotation(id))?;
                let AnnotationKind::FreeText(value) = &mut annotation.content else {
                    return Err(CoreError::IncompatibleEdit(id));
                };
                if !value.is_plain() {
                    return Err(CoreError::UnsupportedFreeText(id));
                }
                if !annotation.metadata.can_edit_contents() {
                    return Err(CoreError::AnnotationNotEditable(id));
                }
                // Replacing /Contents with encoded PDF bytes is obsolete here: live
                // text is Unicode, and PDF serialization belongs to a future exporter.
                value.text = text;
                Change::Updated(id)
            }
            Operation::Translate { id, delta } => {
                let annotation = self
                    .data
                    .annotations
                    .iter_mut()
                    .find(|annotation| annotation.id == id)
                    .ok_or(CoreError::MissingAnnotation(id))?;
                if !annotation.metadata.can_translate() {
                    return Err(CoreError::AnnotationNotEditable(id));
                }
                let mut content = annotation.content.clone();
                content.translate(delta)?;
                annotation.content = content;
                Change::Updated(id)
            }
            Operation::ReplaceContent { id, content } => {
                let annotation = self
                    .view()
                    .annotation(id)
                    .ok_or(CoreError::MissingAnnotation(id))?;
                let editable = if matches!(annotation.content, AnnotationKind::Widget(_)) {
                    annotation.metadata.can_translate()
                } else {
                    annotation.metadata.can_edit_contents() && annotation.metadata.can_translate()
                };
                if !editable {
                    return Err(CoreError::AnnotationNotEditable(id));
                }
                if matches!(&annotation.content, AnnotationKind::FreeText(v) if !v.is_plain())
                    || matches!(&content, AnnotationKind::FreeText(v) if !v.is_plain())
                {
                    return Err(CoreError::UnsupportedFreeText(id));
                }
                if std::mem::discriminant(&annotation.content) != std::mem::discriminant(&content) {
                    return Err(CoreError::IncompatibleEdit(id));
                }
                if let (AnnotationKind::Widget(old), AnnotationKind::Widget(new)) =
                    (&annotation.content, &content)
                    && old.binding != new.binding
                {
                    return Err(CoreError::WidgetRebindingNotAllowed(id));
                }
                if let (AnnotationKind::Widget(old), AnnotationKind::Widget(new)) =
                    (&annotation.content, &content)
                    && old.style != new.style
                {
                    return Err(CoreError::IncompatibleEdit(id));
                }
                Self::validate_annotation(&self.data, annotation.page, &content)?;
                if matches!(&content, AnnotationKind::Popup(v) if v.parent == Some(id)) {
                    return Err(CoreError::InvalidPopupParent(id));
                }
                let annotation = self
                    .data
                    .annotations
                    .iter_mut()
                    .find(|annotation| annotation.id == id)
                    .ok_or(CoreError::MissingAnnotation(id))?;
                annotation.content = content;
                Change::Updated(id)
            }
            Operation::Delete { id } => {
                let annotation = self
                    .view()
                    .annotation(id)
                    .ok_or(CoreError::MissingAnnotation(id))?;
                if !annotation.metadata.can_delete() {
                    return Err(CoreError::AnnotationNotEditable(id));
                }
                if self.data.annotations.iter().any(|a| matches!(&a.content, AnnotationKind::Popup(popup) if popup.parent == Some(id))) {
                    return Err(CoreError::AnnotationInUse(id));
                }
                self.data
                    .annotations
                    .retain(|annotation| annotation.id != id);
                Change::Deleted(id)
            }
            Operation::CreateField { field } => {
                crate::validation::validate(&field)?;
                field.content.validate().map_err(ValidationError::from)?;
                let next = self
                    .data
                    .next_field_id
                    .checked_add(1)
                    .ok_or(CoreError::CounterExhausted)?;
                let id = FieldId(self.data.next_field_id);
                self.data.fields.push(Field {
                    id,
                    definition: field,
                });
                self.data.next_field_id = next;
                Change::FieldCreated(id)
            }
            Operation::DeleteField { field } => {
                if self.view().field(field).is_none() {
                    return Err(CoreError::MissingField(field));
                }
                if self.data.annotations.iter().any(|annotation| {
                    matches!(&annotation.content,
                    AnnotationKind::Widget(widget) if widget.binding.field() == field)
                }) {
                    return Err(CoreError::FieldInUse(field));
                }
                self.data.fields.retain(|candidate| candidate.id != field);
                Change::FieldDeleted(field)
            }
            operation @ (Operation::SetText { field, .. }
            | Operation::SetCheckbox { field, .. }
            | Operation::SetCheckboxGroupSelection { field, .. }
            | Operation::SetRadioSelection { field, .. }
            | Operation::SetListSelection { field, .. }
            | Operation::SetComboValue { field, .. }) => self.edit_field(field, operation)?,
        };
        // Equivalent edits also commit: hosts use this revision, not changed-value counts.
        self.data.revision = revision;
        Ok(Event { revision, change })
    }

    /// Borrow a read-only view; its lifetime prevents concurrent engine edits.
    pub fn view(&self) -> DocumentView<'_> {
        DocumentView {
            data: &self.data,
            last_saved_revision: self.last_saved_revision,
        }
    }

    /// Copy the current document into an owned snapshot without asserting storage.
    ///
    /// This explicit allocation is suitable for export; use [`Self::view`] for
    /// routine rendering. Even an unchanged saved document yields `Unsaved`
    /// because this operation itself does not acknowledge a storage write.
    pub fn snapshot(&self) -> Snapshot<Unsaved> {
        Snapshot {
            data: self.data.clone(),
            state: PhantomData,
        }
    }

    /// Serialize schema-six JSON and atomically save the exact current snapshot.
    ///
    /// Only store acknowledgement advances the last-saved revision and produces
    /// a persisted proof. Serialization or storage errors preserve pending edits
    /// and the previous last-saved revision. Later edits do not alter this proof's
    /// data, and make the engine dirty again. An unchanged document may be saved.
    /// Incomplete required fields are saved as drafts without error.
    ///
    /// # Errors
    /// Returns [`CoreError::Serialization`] if encoding fails or
    /// [`CoreError::Storage`] if the store does not acknowledge the write.
    /// This function does not panic.
    pub fn save(&mut self) -> Result<Snapshot<Persisted>, CoreError<S::Error>> {
        let snapshot = self.snapshot();
        let bytes = serde_json::to_vec(snapshot.data())?;
        self.store
            .save(&snapshot.data.document.id, &bytes)
            .map_err(CoreError::Storage)?;
        self.last_saved_revision = Some(snapshot.data.revision);
        Ok(Snapshot {
            data: snapshot.data,
            state: PhantomData,
        })
    }

    fn validate_annotation(
        data: &DocumentData,
        page: PageIndex,
        content: &AnnotationKind,
    ) -> Result<(), CoreError<S::Error>> {
        if page.0 >= data.document.page_count {
            return Err(ValidationError::InvalidPage {
                page,
                page_count: data.document.page_count,
            }
            .into());
        }
        content.validate()?;
        if let AnnotationKind::Popup(popup) = content
            && let Some(parent) = popup.parent
        {
            let parent = data
                .annotations
                .iter()
                .find(|a| a.id == parent)
                .ok_or(CoreError::InvalidPopupParent(parent))?;
            if parent.page != page || matches!(parent.content, AnnotationKind::Popup(_)) {
                return Err(CoreError::InvalidPopupParent(parent.id));
            }
        }
        if let AnnotationKind::Widget(widget) = content {
            let id = widget.binding.field();
            let field = data
                .fields
                .iter()
                .find(|field| field.id == id)
                .ok_or(CoreError::MissingField(id))?;
            widget.binding.validate(field)?;
        }
        Ok(())
    }

    fn validate_document(data: &DocumentData) -> Result<(), CoreError<S::Error>> {
        let mut fields = std::collections::HashSet::new();
        if data.next_field_id == 0 {
            return Err(ValidationError::InvalidNextFieldId.into());
        }
        for field in &data.fields {
            crate::validation::validate(&field.definition)?;
            if !fields.insert(field.id.0) {
                return Err(ValidationError::DuplicateFieldId(field.id).into());
            }
            if field.id.0 >= data.next_field_id {
                return Err(ValidationError::InvalidNextFieldId.into());
            }
            field
                .definition
                .content
                .validate()
                .map_err(ValidationError::from)?;
        }
        let mut annotations = std::collections::HashSet::new();
        if data.next_annotation_id == 0 {
            return Err(ValidationError::InvalidNextId.into());
        }
        for annotation in &data.annotations {
            annotation.metadata.validate()?;
            if !annotations.insert(annotation.id.0) {
                return Err(ValidationError::DuplicateId(annotation.id).into());
            }
            if annotation.id.0 >= data.next_annotation_id {
                return Err(ValidationError::InvalidNextId.into());
            }
            Self::validate_annotation(data, annotation.page, &annotation.content)?;
        }
        Ok(())
    }

    fn edit_field(
        &mut self,
        id: FieldId,
        operation: Operation,
    ) -> Result<Change, CoreError<S::Error>> {
        let field = self
            .data
            .fields
            .iter_mut()
            .find(|field| field.id == id)
            .ok_or(CoreError::MissingField(id))?;
        if field.definition.read_only {
            return Err(CoreError::ReadOnlyField(id));
        }
        // Configuration is already validated and immutable. Validate only the
        // proposed value, borrowing options instead of cloning their labels/exports.
        match (&mut field.definition.content, operation) {
            (FieldKind::Text(text), Operation::SetText { value, .. }) => {
                text.validate_value(&value).map_err(ValidationError::from)?;
                text.value = value;
            }
            (
                FieldKind::CheckboxGroup(group),
                Operation::SetCheckboxGroupSelection { selected, .. },
            ) => {
                group
                    .validate_selection(selected)
                    .map_err(ValidationError::from)?;
                group.selected = selected;
            }
            (FieldKind::Checkbox(checkbox), Operation::SetCheckbox { checked, .. }) => {
                checkbox.checked = checked
            }
            (FieldKind::Radio(radio), Operation::SetRadioSelection { selected, .. }) => {
                if selected.is_none() && radio.selected.is_some() && !radio.allow_clear {
                    return Err(CoreError::RadioClearNotAllowed(id));
                }
                radio
                    .validate_selection(selected)
                    .map_err(ValidationError::from)?;
                radio.selected = selected;
            }
            (FieldKind::ListBox(list), Operation::SetListSelection { selection, .. }) => {
                list.validate_selection(&selection)
                    .map_err(ValidationError::from)?;
                list.selection = selection;
            }
            (FieldKind::ComboBox(combo), Operation::SetComboValue { value, .. }) => {
                combo
                    .validate_value(&value)
                    .map_err(ValidationError::from)?;
                combo.value = value
            }
            _ => return Err(CoreError::IncompatibleFieldCommand(id)),
        }
        // Shared fields obsolete copying /V into every widget; events refresh all views.
        let widgets = self
            .data
            .annotations
            .iter()
            .filter_map(|annotation| match &annotation.content {
                AnnotationKind::Widget(widget) if widget.binding.field() == id => {
                    Some(annotation.id)
                }
                _ => None,
            })
            .collect();
        Ok(Change::FieldUpdated { field: id, widgets })
    }
}
