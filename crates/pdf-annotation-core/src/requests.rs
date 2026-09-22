//! Typed browser envelopes around Core's existing semantic commands.
use crate::{commands::AnnotationCommand, events::Event, models::AnnotationId};
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

/// Acknowledgement of an applied optional content visibility change.
///
/// Carries no revision: optional content visibility is host presentation state,
/// so applying it never edits an annotation and never advances the document
/// revision. Only the listed pages need their presentation refreshed.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct OptionalContentReceipt {
    /// Pages whose annotation presentation must be refreshed.
    pub pages: Vec<u32>,
}
