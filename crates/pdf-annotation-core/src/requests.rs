//! Typed browser envelopes around Core's existing semantic commands.
use crate::{
    commands::AnnotationCommand,
    events::Event,
    models::{AnnotationId, Revision},
    pdf_data::AnnotationAction,
};
use serde::{Deserialize, Serialize};

/// Captured mounted control context, separate from the document revision.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(deny_unknown_fields)]
pub struct AnnotationTarget {
    /// Page owning the mounted control.
    pub page: u32,
    /// Core identity rendered by the control.
    pub annotation_id: AnnotationId,
    /// Viewport revision captured when the draft or drag began.
    pub viewport_revision: u32,
}

/// Browser context for a Core command; omit `target` for programmatic operations.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(deny_unknown_fields)]
pub struct AnnotationCommandRequest {
    /// Optional mounted control context to validate before dispatch.
    pub target: Option<AnnotationTarget>,
    /// Core command; flattened to retain the browser JSON wire shape.
    #[serde(flatten)]
    pub command: AnnotationCommand,
}

/// Acknowledgement of a committed in-memory edit, never a storage acknowledgement.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct AnnotationReceipt {
    /// Core event containing the new document revision.
    pub event: Event,
    /// Pages whose annotation presentation must be refreshed.
    pub pages: Vec<u32>,
}

/// Outcome of running an annotation's action for one trigger.
///
/// Hide actions commit one Core edit and report its revision. Optional content
/// changes are presentation state and advance no revision. Actions Core does not
/// run are returned for the host, in order.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct ActionReceipt {
    /// Document revision after the committed hide edit; `None` when nothing was edited.
    pub revision: Option<Revision>,
    /// Pages whose annotation presentation must be refreshed.
    pub pages: Vec<u32>,
    /// Actions the host performs itself, such as navigation, in execution order.
    pub host_actions: Vec<AnnotationAction>,
}
