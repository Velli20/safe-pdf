//! Wire data is deliberately distinct from engine-owned, validated state.

use serde::{Deserialize, Serialize};

use crate::fields::Field;
use crate::kind::AnnotationKind;
use pdf_graphics::{color::Color, polyline::Polyline};

/// Current sidecar schema; version six stores live annotation bounds as normalized rectangle edges.
pub const SCHEMA_VERSION: u32 = 6;

/// Host-supplied identity associating a sidecar with one PDF document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct DocumentId(pub String);

/// Engine-allocated identity, unique within a document and never reused there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct AnnotationId(
    #[serde(with = "crate::wire")]
    #[cfg_attr(feature = "typescript", ts(type = "string"))]
    pub u64,
);

/// Zero-based index into the host document's pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct PageIndex(pub u32);

/// Monotonically increasing document edit revision; overflow must be rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Revision(
    #[serde(with = "crate::wire")]
    #[cfg_attr(feature = "typescript", ts(type = "string"))]
    pub u64,
);

/// Position in unrotated PDF page user space, before any viewport transform.
pub type Point = pdf_graphics::point::Point<f64>;

/// Convex highlight region with four distinct corners in perimeter order.
pub type Quad = pdf_graphics::quad::Quad<f64>;

/// Selected regions supplied by the host's text-selection system.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Highlight {
    /// Optional precise live style, taking precedence over compact color/width.
    /// Content replacement replaces this style too; retained source never overrides it.
    #[serde(default)]
    pub style: Option<crate::style::ResolvedStyle>,
    /// Nonempty collection of nondegenerate selection quadrilaterals.
    pub quads: Vec<Quad>,
    /// Fill color used by the host renderer; alpha is the annotation opacity.
    pub color: Color,
}

/// Text attached to an anchor; the host chooses the note widget and appearance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct TextNote {
    /// Live note icon, open/state/intent, and extension metadata, when supplied.
    #[serde(default)]
    pub properties: Option<Box<crate::pdf_data::TextAnnotation>>,
    /// Optional precise live style, taking precedence over compact color/width.
    /// Content replacement replaces this style too; retained source never overrides it.
    #[serde(default)]
    pub style: Option<crate::style::ResolvedStyle>,
    /// Position of the note anchor on its page.
    pub anchor: Point,
    /// Unicode note content; empty notes are allowed.
    pub text: String,
    /// Suggested color for the note marker; alpha is the annotation opacity.
    pub color: Color,
}

/// Completed freehand strokes; collecting pointer samples belongs to the GUI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Ink {
    /// Optional precise live style, taking precedence over compact color/width.
    /// Content replacement replaces this style too; retained source never overrides it.
    #[serde(default)]
    pub style: Option<crate::style::ResolvedStyle>,
    /// Nonempty open strokes, each containing at least two points.
    pub strokes: Vec<Polyline<f64>>,
    /// Finite, strictly positive stroke width in page user-space units.
    pub width: f64,
    /// Stroke color; alpha is the annotation opacity.
    pub color: Color,
}
/// Creation input without an identity, which only Core allocates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct NewAnnotation {
    /// Live capabilities and retained source information; defaults suit new overlays.
    #[serde(default)]
    pub metadata: crate::metadata::AnnotationMetadata,
    /// Page receiving the annotation.
    pub page: PageIndex,
    /// Geometry, content, and styling to validate before insertion.
    pub content: AnnotationKind,
}

/// Owned annotation record used in snapshots and borrowed GUI views.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Annotation {
    /// Live capabilities and retained source information; defaults suit new overlays.
    #[serde(default)]
    pub metadata: crate::metadata::AnnotationMetadata,
    /// Stable identity within the containing document.
    pub id: AnnotationId,
    /// Page containing all of this annotation's geometry.
    pub page: PageIndex,
    /// Typed annotation payload.
    pub content: AnnotationKind,
}

/// Host knowledge used to validate sidecar association and page references.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct DocumentDescriptor {
    /// Stable identity supplied by the host, such as its document fingerprint.
    pub id: DocumentId,
    /// Number of pages; annotation indices must be smaller than this value.
    pub page_count: u32,
}

/// Versioned sidecar DTO; constructing or deserializing it does not validate it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct DocumentData {
    /// Supported wire schema is [`SCHEMA_VERSION`]; older schemas need migration.
    pub schema_version: u32,
    /// Document association checked against the host when loading.
    pub document: DocumentDescriptor,
    /// Revision represented by this complete snapshot.
    pub revision: Revision,
    /// Next unused ID, greater than every allocated ID, including deleted IDs.
    #[serde(with = "crate::wire")]
    #[cfg_attr(feature = "typescript", ts(type = "string"))]
    pub next_annotation_id: u64,
    /// Next unused field ID, greater than all allocated field IDs, including deleted IDs.
    #[serde(with = "crate::wire")]
    #[cfg_attr(feature = "typescript", ts(type = "string"))]
    pub next_field_id: u64,
    /// Shared fields in insertion order with unique identities; unused fields are allowed.
    pub fields: Vec<Field>,
    /// Annotation records in stable insertion order, with unique identities.
    pub annotations: Vec<Annotation>,
}

pub(crate) fn validate_point(point: Point) -> Result<(), crate::error::ValidationError> {
    if point.is_finite() {
        Ok(())
    } else {
        Err(crate::error::ValidationError::NonFiniteGeometry)
    }
}

pub(crate) fn validate_quad(quad: &Quad) -> Result<(), crate::error::ValidationError> {
    for corner in &quad.corners {
        validate_point(*corner)?;
    }
    if quad.is_convex() {
        Ok(())
    } else {
        Err(crate::error::ValidationError::InvalidHighlight)
    }
}

impl Annotation {
    /// Retained PDF record this annotation was adopted from; absent for host-created ones.
    pub fn source(&self) -> Option<&crate::pdf_data::SourceAnnotation> {
        self.metadata.source.as_deref()
    }

    /// Whether live flags permit display and the payload has geometry to place.
    /// Unknown subtypes without usable bounds are never shown.
    pub fn is_visible(&self) -> bool {
        self.metadata.is_visible()
            && !matches!(&self.content, AnnotationKind::Unknown(v) if v.bounds.is_none())
    }

    /// Whether this widget may edit `field`'s value: the field is not read-only and
    /// live flags permit content edits. False for non-widget payloads.
    pub fn can_edit_field(&self, field: &Field) -> bool {
        matches!(self.content, AnnotationKind::Widget(_))
            && !field.definition.read_only
            && self.metadata.can_edit_contents()
    }

    /// Current host-owned action: a link's own navigation, else the metadata action.
    /// Returning it never executes it.
    pub fn action(&self) -> Option<crate::pdf_data::AnnotationAction> {
        match &self.content {
            AnnotationKind::Link(link) => link.resolved_action(),
            _ => self.metadata.action.as_deref().cloned(),
        }
    }

    /// Whether live flags and subtype permit legacy-style interactive dragging.
    /// Core's existing highlight and widget translation capability is preserved.
    pub fn can_translate(&self) -> bool {
        self.metadata.can_translate()
            && !matches!(
                self.content,
                AnnotationKind::Link(_)
                    | AnnotationKind::Popup(_)
                    | AnnotationKind::Caret(_)
                    | AnnotationKind::Unknown(_)
                    | AnnotationKind::Underline(_)
                    | AnnotationKind::Squiggly(_)
                    | AnnotationKind::StrikeOut(_)
            )
    }

    /// Whether plain FreeText content can be edited without changing style or geometry.
    /// Checks live visibility/read-only/locked-contents flags.
    pub fn can_edit_free_text(&self) -> bool {
        self.metadata.can_edit_contents()
            && matches!(&self.content, AnnotationKind::FreeText(value) if value.is_plain())
    }
}
