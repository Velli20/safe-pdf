//! Optional content group visibility applied to annotation presentation.
//!
//! The catalog's `/OCProperties` seeds which groups start visible; an annotation's
//! `/OC` entry names the group or membership dictionary its display depends on.
//! Visibility is host presentation state: it never enters the sidecar and never
//! advances the document revision.
use crate::error::SourceDecodeError;
use crate::ocg_state::{OcgStateEntry, OcgTarget};
use std::collections::{BTreeMap, BTreeSet};

/// Identity of one optional content group within its source document.
///
/// The indirect object number is the only stable identity a PDF gives an optional
/// content group, so it is the identity used here. A group reached only as a direct
/// dictionary has no object number and retains its `/Name` instead, matching what
/// [`OcgTarget::Dictionary`] preserves.
///
/// The generation number is deliberately **not** part of this identity. A document
/// produced by incremental update can name the same logical group as `12 0 R` in
/// `/OCProperties` and `12 1 R` in a `SetOCGState` action; keying on the pair would
/// silently fail to match those and the action would appear to do nothing. Within one
/// effective cross-reference table an object number is already unique.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum OptionalContentGroupId {
    /// A group identified by the indirect object number of its `/OCG` dictionary.
    Object {
        /// PDF object number, distinct from Core annotation and field identities.
        #[serde(with = "crate::wire")]
        #[cfg_attr(feature = "typescript", ts(type = "string"))]
        number: u64,
    },
    /// A group that has no object number, identified by its `/Name` bytes.
    Name {
        /// The group's `/Name` bytes.
        name: Vec<u8>,
    },
}

/// The `/P` visibility policy of an `/OCMD` membership dictionary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum OptionalContentPolicy {
    /// Visible when at least one member group is on. The default policy.
    #[default]
    AnyOn,
    /// Visible only when every member group is on.
    AllOn,
    /// Visible when at least one member group is off.
    AnyOff,
    /// Visible only when every member group is off.
    AllOff,
}

/// Reads a `/P` policy name. An absent entry is the `AnyOn` default and never
/// reaches this conversion.
impl TryFrom<&[u8]> for OptionalContentPolicy {
    type Error = SourceDecodeError;

    fn try_from(name: &[u8]) -> Result<Self, Self::Error> {
        match name {
            b"AnyOn" => Ok(Self::AnyOn),
            b"AllOn" => Ok(Self::AllOn),
            b"AnyOff" => Ok(Self::AnyOff),
            b"AllOff" => Ok(Self::AllOff),
            other => Err(SourceDecodeError::InvalidEntry {
                entry: b"P",
                reason: format!("unsupported optional content policy '{other:?}'"),
            }),
        }
    }
}

/// Owned `/OCMD` membership dictionary retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct OptionalContentMembership {
    /// The `/OCGs` member groups. A single group reference is retained as a
    /// one-element list. Null members are dropped rather than retained.
    pub groups: Vec<OptionalContentGroupId>,
    /// The `/P` policy; `AnyOn` when the entry is absent.
    pub policy: OptionalContentPolicy,
    /// Whether a `/VE` visibility expression was present. Expressions are not
    /// evaluated, and content carrying one is treated as visible. Retaining the
    /// flag lets a host report the limitation without decoding the entry again.
    pub has_visibility_expression: bool,
}

/// Owned `/OC` entry of an annotation: a single group, or a membership dictionary.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum AnnotationOptionalContent {
    /// A direct `/OCG` group reference.
    Group(OptionalContentGroupId),
    /// An `/OCMD` membership dictionary.
    Membership(Box<OptionalContentMembership>),
}

impl AnnotationOptionalContent {
    /// Whether any group this content's visibility depends on appears in `changed`.
    ///
    /// Callers use this to refresh only the pages a state change can affect.
    pub fn depends_on(&self, changed: &BTreeSet<OptionalContentGroupId>) -> bool {
        match self {
            Self::Group(id) => changed.contains(id),
            Self::Membership(membership) => membership.groups.iter().any(|id| changed.contains(id)),
        }
    }
}

/// The `/BaseState` every group starts at before `/ON` and `/OFF` are applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptionalContentBaseState {
    /// `/ON`: groups start visible. The default base state.
    #[default]
    On,
    /// `/OFF`: groups start hidden.
    Off,
}

/// One optional content group declared by the document.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionalContentGroup {
    /// The group's identity.
    pub id: OptionalContentGroupId,
    /// The group's `/Name`. Nominally required, but tolerated absent so an
    /// unreadable name never discards an otherwise usable group.
    pub name: Option<Vec<u8>>,
}

/// The `/D` default optional content configuration. Alternate `/Configs` are not
/// retained, and neither are `/Order`, `/RBGroups`, `/ListMode`, `/AS` or `/Locked`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptionalContentConfiguration {
    /// The `/BaseState` applied to every declared group.
    pub base_state: OptionalContentBaseState,
    /// The `/ON` groups, turned on after the base state.
    pub on: Vec<OptionalContentGroupId>,
    /// The `/OFF` groups, turned off after the base state and `/ON`.
    pub off: Vec<OptionalContentGroupId>,
}

/// The catalog's `/OCProperties` entry, retained without resolving group resources.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptionalContentProperties {
    /// Every group the document declares in `/OCGs`, in source order.
    pub groups: Vec<OptionalContentGroup>,
    /// The `/D` default configuration.
    pub default_configuration: OptionalContentConfiguration,
}

/// Runtime visibility of a document's optional content groups.
///
/// This is disposable presentation state owned by the annotation overlay. It is
/// seeded from [`OptionalContentProperties`] and mutated by `SetOCGState` actions;
/// it is never serialized into the sidecar and never advances a document revision.
#[derive(Clone, Debug, Default)]
pub struct OptionalContentState {
    /// Current visibility of every tracked group. A group absent from this map is
    /// visible, so a document that declares no optional content hides nothing.
    visible: BTreeMap<OptionalContentGroupId, bool>,
    /// Group `/Name` to the identities carrying it, so a `SetOCGState` naming a
    /// direct dictionary can reach groups the catalog declared by object number.
    /// One name may legitimately map to several groups.
    named: BTreeMap<Vec<u8>, Vec<OptionalContentGroupId>>,
}

impl OptionalContentState {
    /// Seeds visibility from a document's `/OCProperties`.
    ///
    /// Every declared group starts at `/BaseState`, then `/ON` turns groups on and
    /// `/OFF` turns them off. `/OFF` is applied last, so a group listed in both is
    /// hidden; the specification leaves that contradiction undefined. A group named
    /// only by `/ON` or `/OFF` is still tracked.
    pub fn from_properties(properties: &OptionalContentProperties) -> Self {
        let configuration = &properties.default_configuration;
        let base = matches!(configuration.base_state, OptionalContentBaseState::On);
        let mut state = Self::default();
        for group in &properties.groups {
            state.visible.insert(group.id.clone(), base);
            if let Some(name) = &group.name {
                state
                    .named
                    .entry(name.clone())
                    .or_default()
                    .push(group.id.clone());
            }
        }
        for id in &configuration.on {
            state.visible.insert(id.clone(), true);
        }
        for id in &configuration.off {
            state.visible.insert(id.clone(), false);
        }
        state
    }

    /// Whether an annotation carrying `content` is currently displayed.
    ///
    /// An `/OCMD` with no member groups imposes no restriction, and one carrying a
    /// `/VE` visibility expression is treated as visible because the expression
    /// takes precedence over `/OCGs` and is not evaluated here.
    pub fn is_visible(&self, content: &AnnotationOptionalContent) -> bool {
        match content {
            AnnotationOptionalContent::Group(id) => self.group_visible(id),
            AnnotationOptionalContent::Membership(membership) => {
                if membership.has_visibility_expression || membership.groups.is_empty() {
                    return true;
                }
                let mut groups = membership.groups.iter().map(|id| self.group_visible(id));
                match membership.policy {
                    OptionalContentPolicy::AnyOn => groups.any(|on| on),
                    OptionalContentPolicy::AllOn => groups.all(|on| on),
                    OptionalContentPolicy::AnyOff => groups.any(|on| !on),
                    OptionalContentPolicy::AllOff => groups.all(|on| !on),
                }
            }
        }
    }

    /// Applies a `SetOCGState` `/State` sequence, returning the groups whose
    /// visibility actually changed.
    ///
    /// Each operation governs the group targets that follow it. A target naming a
    /// group the document never declared is tracked from here on, so an action can
    /// still hide a group missing from `/OCProperties`.
    pub fn apply(&mut self, state: &[OcgStateEntry]) -> BTreeSet<OptionalContentGroupId> {
        let mut changed = BTreeSet::new();
        let mut operation = None;
        for entry in state {
            match entry {
                OcgStateEntry::On => operation = Some(OcgStateEntry::On),
                OcgStateEntry::Off => operation = Some(OcgStateEntry::Off),
                OcgStateEntry::Toggle => operation = Some(OcgStateEntry::Toggle),
                // Decoding rejects a target that precedes any operation, so an
                // absent operation here is unreachable rather than an error.
                OcgStateEntry::Group(target) => {
                    let Some(operation) = &operation else {
                        continue;
                    };
                    for id in self.resolve(target) {
                        let value = match operation {
                            OcgStateEntry::Off => false,
                            OcgStateEntry::Toggle => !self.group_visible(&id),
                            _ => true,
                        };
                        if self.visible.insert(id.clone(), value) != Some(value) {
                            changed.insert(id);
                        }
                    }
                }
            }
        }
        changed
    }

    /// Whether one group is on. An untracked group is visible, the safe default for
    /// a document whose `/OCProperties` never declared it.
    fn group_visible(&self, id: &OptionalContentGroupId) -> bool {
        self.visible.get(id).copied().unwrap_or(true)
    }

    /// The identities a `SetOCGState` target names. A reference names exactly one
    /// group; a direct dictionary names every declared group sharing its `/Name`,
    /// falling back to a name identity when the document declared none.
    fn resolve(&self, target: &OcgTarget) -> Vec<OptionalContentGroupId> {
        match target {
            OcgTarget::Reference { number, .. } => {
                vec![OptionalContentGroupId::Object { number: *number }]
            }
            OcgTarget::Dictionary { name } => match self.named.get(name) {
                Some(ids) => ids.clone(),
                None => vec![OptionalContentGroupId::Name { name: name.clone() }],
            },
        }
    }
}
