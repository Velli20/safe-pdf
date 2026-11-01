//! Optional content group state retained from `SetOCGState` actions.

use std::sync::Arc;

/// Owned optional content group target of a `SetOCGState` `/State` element.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum OcgTarget {
    /// A group reference into the containing source PDF. The referenced group
    /// resource is not resolved, preserving the reference identity.
    Reference {
        /// PDF object number of the group's dictionary.
        #[serde(with = "crate::wire")]
        #[cfg_attr(feature = "typescript", ts(type = "string"))]
        number: u64,
        /// PDF generation number.
        #[serde(with = "crate::wire")]
        #[cfg_attr(feature = "typescript", ts(type = "string"))]
        generation: u64,
    },
    /// A direct `/OCG` dictionary; only its `/Name` is retained.
    Dictionary {
        /// The group's `/Name` bytes.
        name: Arc<[u8]>,
    },
}

/// Optional content group operation of a `SetOCGState` `/State` array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum OcgOperation {
    /// `/ON`: the governed groups are turned on.
    On,
    /// `/OFF`: the governed groups are turned off.
    Off,
    /// `/Toggle`: the governed groups have their state reversed.
    Toggle,
}

/// One operation of a `SetOCGState` `/State` array with the group targets that follow it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct OcgStateChange {
    /// The operation applied to every group in `groups`.
    pub operation: OcgOperation,
    /// The group targets the operation governs, in source order; never empty.
    pub groups: Vec<OcgTarget>,
}

/// Owned `/State` array of a `SetOCGState` action.
///
/// The array is a flat sequence in which each operation governs the group targets
/// that follow it; it is retained as those changes in source order.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct OcgState {
    /// The changes in the order they are applied.
    pub changes: Vec<OcgStateChange>,
}
