use serde::{Deserialize, Serialize};

use crate::fields::FieldId;

use crate::models::{AnnotationId, Revision};

/// A committed edit result; failures produce an error instead of an event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Event {
    /// Document revision after the edit, exactly one beyond its prior revision.
    pub revision: Revision,
    /// Identity and nature of the change; inspect the engine for current content.
    pub change: Change,
}

/// Invalidation information for the GUI's annotation presentation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(tag = "change", content = "data", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Change {
    /// Core assigned this identity to a newly inserted annotation.
    Created(AnnotationId),
    /// Existing annotation geometry or content was replaced.
    Updated(AnnotationId),
    /// The annotation is no longer present in the live document.
    Deleted(AnnotationId),
    /// Core assigned this identity to a new shared field.
    FieldCreated(FieldId),
    /// An unreferenced field was removed from the document.
    FieldDeleted(FieldId),
    /// A field value command committed; all bound widgets should be refreshed.
    FieldUpdated {
        /// Shared field whose value command was accepted.
        field: FieldId,
        /// Every referencing widget ID, without duplicates, in annotation insertion order.
        /// Includes all radio options and repeated views; empty for an unplaced field.
        widgets: Vec<AnnotationId>,
    },
}
