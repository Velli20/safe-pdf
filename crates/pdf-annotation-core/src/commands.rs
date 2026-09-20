use serde::{Deserialize, Serialize};

use crate::fields::{ComboValue, FieldId, ListSelection, NewField, OptionId};

use crate::kind::AnnotationKind;
use crate::models::{AnnotationId, NewAnnotation, Point, Revision};

/// One optimistic, atomic edit against the engine's current document revision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct AnnotationCommand {
    /// Reject the edit if another accepted command has advanced this revision.
    pub expected_revision: Revision,
    /// Semantic edit already resolved from GUI input and coordinate transforms.
    pub operation: Operation,
}

/// Completed edits; transient dragging and drawing previews remain host-owned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(tag = "operation", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Operation {
    /// Select a checkbox export state, or clear every member of the group.
    SetCheckboxGroupSelection {
        /// Existing writable checkbox group.
        field: FieldId,
        /// Existing option identity, or an unchecked draft.
        selected: Option<OptionId>,
    },
    /// Replace plain FreeText Unicode text while preserving geometry and style.
    SetFreeTextText {
        /// Existing plain FreeText annotation with editable contents.
        id: AnnotationId,
        /// New Unicode text; empty drafts are accepted.
        text: String,
    },
    /// Validate and insert an annotation with a newly allocated identity.
    Create {
        /// Draft data for the new annotation.
        annotation: NewAnnotation,
    },
    /// Move every geometry point while retaining the annotation's page.
    Translate {
        /// Existing annotation to move.
        id: AnnotationId,
        /// Finite displacement in unrotated PDF page user space.
        delta: Point,
    },
    /// Replace geometry, content, and style without changing kind, page, or ID.
    /// Widget replacements must preserve their binding and only change bounds.
    ReplaceContent {
        /// Existing annotation to edit.
        id: AnnotationId,
        /// Replacement payload, which must have the same variant as before.
        content: AnnotationKind,
    },
    /// Remove an existing annotation without recycling its identity.
    /// Removing a widget leaves the referenced shared field intact.
    Delete {
        /// Existing annotation to remove.
        id: AnnotationId,
    },
    /// Validate and insert a shared field, allocating its identity independently.
    CreateField {
        /// Fixed configuration and initial value; incomplete required values are valid.
        field: NewField,
    },
    /// Remove a field only when no widget references it; never recycle its identity.
    DeleteField {
        /// Existing, unreferenced field to remove.
        field: FieldId,
    },
    /// Replace a text field value after checking its mode and character limit.
    SetText {
        /// Existing writable text field; combos use `SetComboValue`.
        field: FieldId,
        /// New Unicode text, including an empty draft.
        value: String,
    },
    /// Set the boolean value displayed by every bound checkbox.
    SetCheckbox {
        /// Existing writable checkbox field.
        field: FieldId,
        /// New checked state; false is allowed even when required.
        checked: bool,
    },
    /// Select a radio option, or clear selection when the field allows it.
    SetRadioSelection {
        /// Existing writable radio field.
        field: FieldId,
        /// Existing option, or `None`; clearing an already empty field is allowed.
        selected: Option<OptionId>,
    },
    /// Replace listbox selection while preserving its single/multiple mode.
    SetListSelection {
        /// Existing writable listbox field.
        field: FieldId,
        /// Existing option identities; reject duplicates and non-option-list ordering.
        selection: ListSelection,
    },
    /// Select a combo option, clear it, or enter text for an editable combo.
    SetComboValue {
        /// Existing writable combo field.
        field: FieldId,
        /// New value; reject unknown options and custom text in closed combos.
        value: ComboValue,
    },
}
