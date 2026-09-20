//! Extended live annotation payloads in page space, independent of native toolkits.
use crate::{
    models::{AnnotationId, Quad},
    pdf_data::{
        AnnotationAction, AnnotationDestination, BorderEffect, LineEndingStyle, LinkHighlightMode,
    },
    style::ResolvedStyle,
};
use pdf_graphics::{polyline::Polyline, rect::Rect};
use serde::{Deserialize, Serialize};

/// Straight line, open polyline, or closed polygon geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct PathAnnotation {
    /// Annotation rectangle; translations move it together with all vertices.
    pub bounds: Rect<f64>,
    /// Ordered vertices; `closed` is true only for polygons and never duplicates a vertex.
    pub vertices: Polyline<f64>,
    /// Optional endpoint decorations, preserving unknown names.
    pub endings: Option<[LineEndingStyle; 2]>,
    /// Current stroke/fill style.
    pub style: ResolvedStyle,
}

/// Rectangular or elliptical shape, with PDF decoration metadata retained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct BoxAnnotation {
    /// Page-space rectangle enclosing the shape.
    pub bounds: Rect<f64>,
    /// Current stroke/fill style.
    pub style: ResolvedStyle,
    /// Optional border effect, which may be unsupported by a host.
    pub border_effect: Option<BorderEffect>,
    /// Optional left, top, right, bottom difference rectangle.
    pub insets: Option<[f64; 4]>,
}

/// Underline, squiggly, or strikeout regions, in Core perimeter order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Markup {
    /// Nonempty convex regions, retaining individual quad boundaries.
    pub quads: Vec<Quad>,
    /// Current stroke/fill style.
    pub style: ResolvedStyle,
}

/// Link bounds and host-owned navigation metadata; Core executes no actions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Link {
    /// Fallback interaction bounds.
    pub bounds: Rect<f64>,
    /// Optional quadrilateral interaction regions in perimeter order.
    pub quads: Option<Vec<Quad>>,
    /// Pointer highlight feedback metadata.
    pub highlight_mode: Option<LinkHighlightMode>,
    /// Destination used when no action is present.
    pub destination: Option<AnnotationDestination>,
    /// Action metadata taking precedence over the destination.
    pub action: Option<AnnotationAction>,
    /// Current border style.
    pub style: ResolvedStyle,
}

impl Link {
    /// The action, or the destination as a go-to action when no action is present.
    pub fn resolved_action(&self) -> Option<AnnotationAction> {
        self.action.clone().or_else(|| {
            self.destination
                .clone()
                .map(|destination| AnnotationAction::GoTo { destination })
        })
    }
}

/// Stamp identity. Custom names remain data without claiming artwork support.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Stamp {
    /// Page-space stamp rectangle.
    pub bounds: Rect<f64>,
    /// Original stamp name bytes.
    pub name: Vec<u8>,
    /// Current style.
    pub style: ResolvedStyle,
}

impl Stamp {
    /// Whether `name` is a legacy-supported standard rubber-stamp label.
    pub fn has_standard_name(&self) -> bool {
        const NAMES: &[&[u8]] = &[
            b"Approved",
            b"Experimental",
            b"NotApproved",
            b"AsIs",
            b"Expired",
            b"NotForPublicRelease",
            b"Confidential",
            b"Final",
            b"Sold",
            b"Departmental",
            b"ForComment",
            b"TopSecret",
            b"Draft",
            b"ForPublicRelease",
        ];
        NAMES.contains(&self.name.as_slice())
    }
}

/// Popup attached to an optional Core annotation on the same page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Popup {
    /// Popup placement.
    pub bounds: Rect<f64>,
    /// Live parent identity, distinct from retained PDF object references.
    pub parent: Option<AnnotationId>,
    /// Initial visibility requested from the host.
    pub open: bool,
    /// Popup text.
    pub text: String,
}

/// Caret annotation retained without claiming native rendering support.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Caret {
    /// Placement rectangle.
    pub bounds: Rect<f64>,
    /// Legacy caret properties.
    pub properties: crate::pdf_data::CaretAnnotation,
}

/// Unknown subtype retained explicitly rather than approximated by another kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct UnknownAnnotation {
    /// Original subtype name bytes.
    pub subtype: Vec<u8>,
    /// Optional usable page-space bounds.
    pub bounds: Option<Rect<f64>>,
}
