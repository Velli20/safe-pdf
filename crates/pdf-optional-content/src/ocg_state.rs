//! Optional content group state retained from `SetOCGState` actions.

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
        name: Vec<u8>,
    },
}

/// Owned `/State` element of a `SetOCGState` action.
///
/// The `/State` array is a flat sequence in which each operation governs the
/// group targets that follow it, so elements are retained in their source order.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum OcgStateEntry {
    /// `/ON`: the groups that follow are turned on.
    On,
    /// `/OFF`: the groups that follow are turned off.
    Off,
    /// `/Toggle`: the groups that follow have their state reversed.
    Toggle,
    /// A group governed by the preceding operation.
    Group(OcgTarget),
}
