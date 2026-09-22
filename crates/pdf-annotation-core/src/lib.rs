//! GUI-independent annotation editing and validated JSON sidecar storage.
//!
//! Commands atomically update validated state; explicit saves acknowledge storage.
//! Hosts own rendering, hit testing, selection, previews,
//! and conversion into unrotated PDF page user-space coordinates.
#![deny(missing_docs)]

/// Semantic input from the GUI, independent of pointer devices and widgets.
pub mod commands;
/// Ownership boundary for validated live state and persistence proofs.
pub mod engine;
/// Typed failures that preserve validation and storage context.
pub mod error;
/// Committed changes returned to the GUI for refreshing its presentation.
pub mod events;
/// Shared form field definitions, typed values, and completeness issues.
pub mod fields;
/// Owned, serializable sidecar data; deserialization does not establish validity.
pub mod models;
/// Host-provided storage independent of any filesystem or database.
pub mod store;
/// Page-level widget geometry and bindings to document-level fields.
pub mod widgets;

/// Styled free-text payloads and native editing constraints.
pub mod free_text;

/// Annotation payload vocabulary with validation, translation, and live bounds.
pub mod kind;
/// Source snapshots and live annotation capabilities.
pub mod metadata;
/// Optional content group state retained from `SetOCGState` actions.
pub mod ocg_state;
/// Owned source PDF annotation types, independent of parser resources.
pub mod pdf_data;
/// Canonical native PDF annotation record and subtype vocabulary.
pub use pdf_data::{
    CaretAnnotation, CircleAnnotation, FreeTextAnnotation, HighlightAnnotation, InkAnnotation,
    LineAnnotation, LinkAnnotation, NativeAnnotation, PolyLineAnnotation, PolygonAnnotation,
    PopupAnnotation, SourceAnnotation, SquareAnnotation, SquigglyAnnotation, StampAnnotation,
    StrikeOutAnnotation, TextAnnotation, UnderlineAnnotation, WidgetAnnotation,
};
/// Device-independent style conversion and interpretation.
pub mod style;
/// Extended annotation geometry and semantic payloads.
pub mod subtypes;
mod validation;

mod source_decode;
mod source_decode_actions;
mod source_decode_subtypes;
mod source_decode_widget;
/// Shared fields normalized from the widget records that display them.
pub mod source_fields;
mod source_kind;
mod source_properties;

mod wire;

/// Backend-neutral annotation entries projected into one host viewport.
pub mod entry;
mod import;
/// Errors at the backend-neutral annotation layer boundary.
pub mod layer_error;
/// Document-owned Core editing with per-viewport prepared entries.
pub mod overlay;
mod projection;
/// Command envelopes and committed edit receipts shared by every host.
pub mod requests;
/// Session-only annotation storage.
pub mod session_store;
/// Local-unit outlines for annotations drawn by hosts.
pub mod shapes;
mod widget_control;

pub use entry::{AnnotationEntry, ControlKind, ControlTag, WidgetControl};
pub use layer_error::{AnnotationLayerError, AnnotationLayerResult};
pub use overlay::{AnnotationOverlay, PreparedAnnotations};
pub use requests::{AnnotationCommandRequest, AnnotationReceipt, AnnotationTarget};
pub use session_store::{SessionStorageError, SessionStore};
pub use shapes::{Shape, ShapePaint};
