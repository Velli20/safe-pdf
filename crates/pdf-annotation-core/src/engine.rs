use std::marker::PhantomData;

use crate::commands::{AnnotationCommand, Operation};
use crate::error::{CoreError, ValidationError};
use crate::events::{Change, Event};
use crate::field_validation::check_selected;
use crate::fields::{Field, FieldId, FieldIssue, FieldIssueKind, FieldKind, NewField};
use crate::kind::AnnotationKind;
use crate::metadata::AnnotationFlags;
use crate::models::{
    Annotation, AnnotationId, DocumentData, DocumentDescriptor, NewAnnotation, Point, Revision,
    SCHEMA_VERSION,
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
        self.data.annotation(id)
    }

    /// Borrow a shared field without exposing mutable state to the host.
    pub fn field(&self, id: FieldId) -> Option<&'a Field> {
        self.data.field(id)
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
        data.validate()?;
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
        let revision = Revision(next_counter(self.data.revision.0)?);
        let change = match command.operation {
            Operation::Create { annotation } => self.create_annotation(annotation)?,
            Operation::SetFreeTextText { id, text } => self.set_free_text_text(id, text)?,
            Operation::Translate { id, delta } => self.translate(id, delta)?,
            Operation::ReplaceContent { id, content } => self.replace_content(id, content)?,
            Operation::Delete { id } => self.delete_annotation(id)?,
            Operation::CreateField { field } => self.create_field(field)?,
            Operation::DeleteField { field } => self.delete_field(field)?,
            Operation::SetHidden { hidden, shown } => self.set_hidden(hidden, shown)?,
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

    /// Borrow an annotation by identity, or report it missing.
    fn annotation(&self, id: AnnotationId) -> Result<&Annotation, CoreError<S::Error>> {
        self.data
            .annotation(id)
            .ok_or(CoreError::MissingAnnotation(id))
    }

    /// Mutably borrow an annotation by identity, or report it missing.
    fn annotation_mut(&mut self, id: AnnotationId) -> Result<&mut Annotation, CoreError<S::Error>> {
        self.data
            .annotation_mut(id)
            .ok_or(CoreError::MissingAnnotation(id))
    }

    /// Borrow a field by identity, or report it missing.
    fn field(&self, id: FieldId) -> Result<&Field, CoreError<S::Error>> {
        self.data.field(id).ok_or(CoreError::MissingField(id))
    }

    /// Mutably borrow a field by identity, or report it missing.
    fn field_mut(&mut self, id: FieldId) -> Result<&mut Field, CoreError<S::Error>> {
        self.data.field_mut(id).ok_or(CoreError::MissingField(id))
    }

    /// Identities of the widgets bound to `field`, in annotation insertion order.
    fn bound_widgets(&self, field: FieldId) -> impl Iterator<Item = AnnotationId> + '_ {
        self.data
            .annotations
            .iter()
            .filter(move |annotation| annotation.content.bound_field() == Some(field))
            .map(|annotation| annotation.id)
    }

    /// Whether any popup names `id` as its parent.
    fn has_popup_child(&self, id: AnnotationId) -> bool {
        self.data
            .annotations
            .iter()
            .any(|annotation| annotation.content.popup_parent() == Some(id))
    }

    /// Validates a draft and inserts it under a newly allocated identity.
    fn create_annotation(
        &mut self,
        annotation: NewAnnotation,
    ) -> Result<Change, CoreError<S::Error>> {
        crate::finite_values::check(&annotation)?;
        self.data
            .validate_content(annotation.page, &annotation.content)?;
        let id = AnnotationId(allocate(&mut self.data.next_annotation_id)?);
        self.data.annotations.push(Annotation {
            metadata: annotation.metadata,
            id,
            page: annotation.page,
            content: annotation.content,
        });
        Ok(Change::Created(id))
    }

    /// Replaces the text of an editable, plain FreeText annotation.
    fn set_free_text_text(
        &mut self,
        id: AnnotationId,
        text: String,
    ) -> Result<Change, CoreError<S::Error>> {
        let annotation = self.annotation_mut(id)?;
        let editable = annotation.can_edit_free_text();
        let AnnotationKind::FreeText(value) = &mut annotation.content else {
            return Err(CoreError::IncompatibleEdit(id));
        };
        if !editable {
            return Err(CoreError::AnnotationNotEditable(id));
        }
        // Replacing /Contents with encoded PDF bytes is obsolete here: live
        // text is Unicode, and PDF serialization belongs to a future exporter.
        value.text = text;
        Ok(Change::Updated(id))
    }

    /// Moves every geometry point, committing only when the whole move succeeds.
    fn translate(&mut self, id: AnnotationId, delta: Point) -> Result<Change, CoreError<S::Error>> {
        let annotation = self.annotation_mut(id)?;
        if !annotation.metadata.can_translate() {
            return Err(CoreError::AnnotationNotEditable(id));
        }
        let mut content = annotation.content.clone();
        content.translate(delta);
        crate::finite_values::check(&content)?;
        content.validate()?;
        annotation.content = content;
        Ok(Change::Updated(id))
    }

    /// Replaces an annotation's payload with one of the same kind.
    ///
    /// Widgets may change bounds only; their binding and style are fixed.
    fn replace_content(
        &mut self,
        id: AnnotationId,
        content: AnnotationKind,
    ) -> Result<Change, CoreError<S::Error>> {
        let annotation = self.annotation(id)?;
        let metadata = &annotation.metadata;
        let is_widget = matches!(annotation.content, AnnotationKind::Widget(_));
        if !(metadata.can_translate() && (is_widget || metadata.can_edit_contents())) {
            return Err(CoreError::AnnotationNotEditable(id));
        }
        if [&annotation.content, &content]
            .into_iter()
            .any(|kind| matches!(kind, AnnotationKind::FreeText(value) if !value.is_plain()))
        {
            return Err(CoreError::UnsupportedFreeText(id));
        }
        if std::mem::discriminant(&annotation.content) != std::mem::discriminant(&content) {
            return Err(CoreError::IncompatibleEdit(id));
        }
        if let (AnnotationKind::Widget(old), AnnotationKind::Widget(new)) =
            (&annotation.content, &content)
        {
            if old.binding != new.binding {
                return Err(CoreError::WidgetRebindingNotAllowed(id));
            }
            if old.style != new.style {
                return Err(CoreError::IncompatibleEdit(id));
            }
        }
        crate::finite_values::check(&content)?;
        self.data.validate_content(annotation.page, &content)?;
        if content.popup_parent() == Some(id) {
            return Err(CoreError::InvalidPopupParent(id));
        }
        self.annotation_mut(id)?.content = content;
        Ok(Change::Updated(id))
    }

    /// Removes a deletable annotation that no popup references.
    fn delete_annotation(&mut self, id: AnnotationId) -> Result<Change, CoreError<S::Error>> {
        if !self.annotation(id)?.metadata.can_delete() {
            return Err(CoreError::AnnotationNotEditable(id));
        }
        if self.has_popup_child(id) {
            return Err(CoreError::AnnotationInUse(id));
        }
        self.data
            .annotations
            .retain(|annotation| annotation.id != id);
        Ok(Change::Deleted(id))
    }

    /// Validates a field definition and inserts it under a newly allocated identity.
    fn create_field(&mut self, field: NewField) -> Result<Change, CoreError<S::Error>> {
        crate::finite_values::check(&field)?;
        field.validate()?;
        let id = FieldId(allocate(&mut self.data.next_field_id)?);
        self.data.fields.push(Field {
            id,
            definition: field,
        });
        Ok(Change::FieldCreated(id))
    }

    /// Removes a field that no widget is bound to.
    fn delete_field(&mut self, field: FieldId) -> Result<Change, CoreError<S::Error>> {
        self.field(field)?;
        if self.bound_widgets(field).next().is_some() {
            return Err(CoreError::FieldInUse(field));
        }
        self.data.fields.retain(|candidate| candidate.id != field);
        Ok(Change::FieldDeleted(field))
    }

    /// Sets the hidden flag of `hidden` and clears it on `shown`, once every
    /// annotation is known to exist.
    fn set_hidden(
        &mut self,
        hidden: Vec<AnnotationId>,
        shown: Vec<AnnotationId>,
    ) -> Result<Change, CoreError<S::Error>> {
        for id in hidden.iter().chain(&shown) {
            self.annotation(*id)?;
        }
        for annotation in &mut self.data.annotations {
            if hidden.contains(&annotation.id) {
                annotation.metadata.flags.insert(AnnotationFlags::HIDDEN);
            } else if shown.contains(&annotation.id) {
                annotation.metadata.flags.remove(AnnotationFlags::HIDDEN);
            }
        }
        Ok(Change::VisibilityUpdated {
            annotations: hidden.into_iter().chain(shown).collect(),
        })
    }

    fn edit_field(
        &mut self,
        id: FieldId,
        operation: Operation,
    ) -> Result<Change, CoreError<S::Error>> {
        let field = self.field_mut(id)?;
        if field.definition.read_only {
            return Err(CoreError::ReadOnlyField(id));
        }
        // Configuration is already validated and immutable. Validate only the
        // proposed value, borrowing options instead of cloning their labels/exports.
        match (&mut field.definition.content, operation) {
            (FieldKind::Text(text), Operation::SetText { value, .. }) => {
                text.validate_value(&value)?;
                text.value = value;
            }
            (
                FieldKind::CheckboxGroup(group),
                Operation::SetCheckboxGroupSelection { selected, .. },
            ) => {
                check_selected(&group.options, selected)?;
                group.selected = selected;
            }
            (FieldKind::Checkbox(checkbox), Operation::SetCheckbox { checked, .. }) => {
                checkbox.checked = checked
            }
            (FieldKind::Radio(radio), Operation::SetRadioSelection { selected, .. }) => {
                if selected.is_none() && radio.selected.is_some() && !radio.allow_clear {
                    return Err(CoreError::RadioClearNotAllowed(id));
                }
                check_selected(&radio.options, selected)?;
                radio.selected = selected;
            }
            (FieldKind::ListBox(list), Operation::SetListSelection { selection, .. }) => {
                list.validate_selection(&selection)?;
                list.selection = selection;
            }
            (FieldKind::ComboBox(combo), Operation::SetComboValue { value, .. }) => {
                combo.validate_value(&value)?;
                combo.value = value
            }
            _ => return Err(CoreError::IncompatibleFieldCommand(id)),
        }
        // Shared fields obsolete copying /V into every widget; events refresh all views.
        let widgets = self.bound_widgets(id).collect();
        Ok(Change::FieldUpdated { field: id, widgets })
    }
}

/// Return the successor of an allocation counter, or report it exhausted.
fn next_counter<E: std::error::Error + 'static>(value: u64) -> Result<u64, CoreError<E>> {
    value.checked_add(1).ok_or(CoreError::CounterExhausted)
}

/// Take the counter's current value as a new identity and advance it past that value.
/// An exhausted counter is left unchanged.
fn allocate<E: std::error::Error + 'static>(counter: &mut u64) -> Result<u64, CoreError<E>> {
    let id = *counter;
    *counter = next_counter(id)?;
    Ok(id)
}
