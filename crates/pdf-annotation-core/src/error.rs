use crate::fields::{FieldId, OptionId};
use crate::models::{AnnotationId, DocumentDescriptor, PageIndex, Revision};

/// Errors while decoding page annotation dictionaries into Core's owned types.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SourceDecodeError {
    /// An error returned while reading or resolving a PDF object.
    #[error("{0}")]
    Object(#[from] pdf_object_reader::object_error::ObjectError),
    /// An annotation entry is present but has the wrong type or value shape.
    #[error("invalid annotation entry '/{entry:?}': {reason}")]
    InvalidEntry {
        /// The dictionary key of the offending entry.
        entry: &'static [u8],
        /// Why the entry was rejected.
        reason: String,
    },
    /// A required annotation entry is missing.
    #[error("missing required annotation entry '/{entry:?}'")]
    MissingEntry {
        /// The dictionary key of the missing entry.
        entry: &'static [u8],
    },
    /// A source identifier cannot be represented by Core.
    #[error("annotation source exceeds Core resource limits")]
    ResourceLimit,
}

impl From<SourceDecodeError> for pdf_object_reader::ObjectReadError {
    fn from(source: SourceDecodeError) -> Self {
        Self::Decode {
            target: "PDF annotation",
            source: Box::new(source),
        }
    }
}

/// Structurally invalid field data, distinct from an incomplete required draft.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FieldValidationError {
    /// An export-state checkbox group must contain an option.
    #[error("checkbox groups require at least one option")]
    EmptyCheckboxOptions,
    /// Option identities within one field must be unique.
    #[error("duplicate field option: {0:?}")]
    DuplicateOption(OptionId),
    /// A value references an option absent from its field.
    #[error("unknown field option: {0:?}")]
    UnknownOption(OptionId),
    /// Radio configuration requires at least one option.
    #[error("radio fields require at least one option")]
    EmptyRadioOptions,
    /// Multiple list selection must contain distinct IDs in option-list order.
    #[error("list selection must be unique and follow option-list order")]
    InvalidListSelection,
    /// A command attempted to switch a listbox's fixed selection cardinality.
    #[error("listbox selection mode cannot change")]
    SelectionModeMismatch,
    /// Unicode scalar count exceeds the field's configured maximum.
    #[error("text exceeds the maximum of {max} Unicode scalar values")]
    TextTooLong {
        /// Field's maximum permitted Unicode scalar count.
        max: u32,
    },
    /// Single-line/password text and combo text cannot contain CR or LF.
    #[error("single-line input cannot contain line breaks")]
    LineBreakNotAllowed,
    /// Closed combo fields cannot hold custom text values.
    #[error("custom text requires an editable combo")]
    CustomTextNotAllowed,
}

/// Invalid untrusted data rejected before it can change live annotation state.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    /// Retained PDF metadata contains a number that JSON cannot preserve.
    #[error("retained metadata numbers must be finite")]
    NonFiniteMetadata,
    /// Source button has no usable non-Off appearance state or checkbox fallback.
    #[error("source button {source_id} has no on-state")]
    MissingButtonOnState {
        /// Historical page-local source annotation identity, not a Core ID.
        source_id: u64,
    },
    /// Source annotation has no `/Rect` although its kind requires one.
    #[error("source annotation {source_id} has no rectangle")]
    MissingRect {
        /// Historical page-local source annotation identity, not a Core ID.
        source_id: u64,
    },
    /// Positive bounds require finite coordinates, dimensions, and maximum corners.
    #[error("annotation bounds must be finite and positive")]
    InvalidBounds,
    /// A path does not have the required vertex count or geometry.
    #[error("invalid annotation path: {field}")]
    InvalidPath {
        /// Geometry property that failed validation.
        field: &'static str,
    },
    /// Retained metadata violates its serializable representation.
    #[error("invalid retained metadata: {field}")]
    InvalidMetadata {
        /// Rejected metadata property.
        field: &'static str,
    },
    /// A coordinate or translation component is not finite.
    #[error("geometry must contain only finite coordinates")]
    NonFiniteGeometry,
    /// Highlights require nonempty, nondegenerate convex quadrilaterals.
    #[error("highlight regions must be nonempty convex quadrilaterals")]
    InvalidHighlight,
    /// Ink requires at least one stroke with at least two points per stroke.
    #[error("ink requires nonempty strokes with at least two points each")]
    InvalidInk,
    /// A retained PDF text string has no Unicode interpretation.
    #[error("undecodable PDF text string: {0}")]
    InvalidTextString(#[from] pdf_object_reader::text_string::TextStringError),
    /// The widgets sharing one source field disagree or describe an unusable field.
    #[error("invalid source field: {reason}")]
    InvalidSourceField {
        /// Why the field group cannot be normalized.
        reason: &'static str,
    },
    /// Width or opacity is outside the documented range or is not finite.
    #[error("invalid annotation style: {field}")]
    InvalidStyle {
        /// Name of the invalid style field.
        field: &'static str,
    },
    /// The annotation references a page outside its document.
    #[error("page {page:?} is outside a document with {page_count} pages")]
    InvalidPage {
        /// Rejected zero-based page index.
        page: PageIndex,
        /// Host-supplied page count.
        page_count: u32,
    },
    /// Deserialized records contain repeated identities.
    #[error("duplicate annotation identity: {0:?}")]
    DuplicateId(AnnotationId),
    /// The snapshot's allocation counter could reuse an existing identity.
    #[error("invalid next annotation identity")]
    InvalidNextId,
    /// Host document identities must not be empty.
    #[error("document identity must not be empty")]
    EmptyDocumentId,
    /// A widget requires positive extents and finite minimum and maximum corners.
    #[error("widget bounds must be finite and have positive extents")]
    InvalidWidgetBounds,
    /// Field content failed validation independently of required-value completeness.
    #[error(transparent)]
    Field(#[from] FieldValidationError),
    /// Sidecar field identities must be unique.
    #[error("duplicate field identity: {0:?}")]
    DuplicateFieldId(FieldId),
    /// Allocation must not recycle existing or previously allocated field IDs.
    #[error("invalid next field identity")]
    InvalidNextFieldId,
    /// A widget's binding kind or radio option is incompatible with its field.
    #[error("widget binding is incompatible with field {0:?}")]
    InvalidWidgetBinding(FieldId),
}

/// Failures shared by command dispatch, loading, and explicit persistence.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CoreError<E: std::error::Error + 'static> {
    /// Live annotation flags prohibit the requested edit.
    #[error("annotation is not editable: {0:?}")]
    AnnotationNotEditable(AnnotationId),
    /// Rich content, callouts, or effects cannot be changed by the plain-text editor.
    #[error("plain free-text editing is unsupported for annotation: {0:?}")]
    UnsupportedFreeText(AnnotationId),
    /// A live popup still references this annotation.
    #[error("annotation is referenced by a popup: {0:?}")]
    AnnotationInUse(AnnotationId),
    /// A popup parent must be another annotation on the same page.
    #[error("invalid popup parent: {0:?}")]
    InvalidPopupParent(AnnotationId),
    /// Input failed validation; the live document remains unchanged.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// A command references an annotation absent from the current document.
    #[error("annotation not found: {0:?}")]
    MissingAnnotation(AnnotationId),
    /// A command or widget binding references a nonexistent field.
    #[error("field not found: {0:?}")]
    MissingField(FieldId),
    /// Field deletion would leave one or more widget bindings dangling.
    #[error("field is still referenced by widgets: {0:?}")]
    FieldInUse(FieldId),
    /// Value commands cannot edit read-only fields, even to the same value.
    #[error("field is read-only: {0:?}")]
    ReadOnlyField(FieldId),
    /// The value command does not match the target field kind.
    #[error("command is incompatible with field {0:?}")]
    IncompatibleFieldCommand(FieldId),
    /// A selected radio group disallows explicit clearing.
    #[error("radio selection cannot be cleared: {0:?}")]
    RadioClearNotAllowed(FieldId),
    /// Widget geometry replacement cannot rebind the widget to another field/option.
    #[error("widget binding cannot change: {0:?}")]
    WidgetRebindingNotAllowed(AnnotationId),
    /// Content replacement attempted to change the annotation kind.
    #[error("replacement changes the kind of annotation {0:?}")]
    IncompatibleEdit(AnnotationId),
    /// The GUI submitted an edit based on an obsolete document revision.
    #[error("revision conflict: expected {expected:?}, current {actual:?}")]
    RevisionConflict {
        /// Revision supplied by the GUI.
        expected: Revision,
        /// Current live revision.
        actual: Revision,
    },
    /// The snapshot uses a schema this engine cannot load.
    #[error("unsupported sidecar schema version: {0}")]
    UnsupportedSchema(u32),
    /// Sidecar identity or page count differs from the host document.
    #[error("sidecar document mismatch: expected {expected:?}, found {actual:?}")]
    DocumentMismatch {
        /// Descriptor supplied by the host.
        expected: DocumentDescriptor,
        /// Descriptor found in storage.
        actual: DocumentDescriptor,
    },
    /// The host requested loading a document with no stored sidecar.
    #[error("no sidecar exists for this document")]
    SidecarMissing,
    /// Annotation/field ID allocation or revision advancement would overflow.
    #[error("annotation identity, field identity, or revision counter exhausted")]
    CounterExhausted,
    /// JSON encoding or decoding failed, preserving its original source.
    #[error("sidecar serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    /// The host storage backend failed; pending edits remain available.
    #[error("annotation storage failed: {0}")]
    Storage(#[source] E),
}
