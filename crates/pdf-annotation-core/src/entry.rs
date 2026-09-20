//! Backend-neutral annotation entries: Core values projected into one viewport.
use crate::shapes::Shape;
use crate::{
    fields::{ChoiceOption, FieldId, OptionId},
    kind::AnnotationKind,
    models::{AnnotationId, Revision},
    pdf_data::AnnotationAction,
    style::ResolvedStyle,
};
use serde::{Deserialize, Serialize};

/// Host element family presenting a widget's field value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(rename_all = "snake_case")]
pub enum ControlTag {
    /// Single-line text, password, checkbox, or radio input.
    Input,
    /// Multiline text input.
    TextArea,
    /// Option list or drop-down.
    Select,
    /// Push button.
    Button,
}

/// Field semantics the host maps back into Core commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    /// Free text value.
    Text,
    /// Independent checkbox.
    Checkbox,
    /// One option of a checkbox group.
    CheckboxGroup,
    /// One option of a radio group.
    Radio,
    /// Scrollable list of options.
    ListBox,
    /// Drop-down, optionally editable.
    ComboBox,
    /// Push button.
    Button,
}

/// Native control derived from a widget's resolved Core field; hosts never read PDF flags.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct WidgetControl {
    /// Element family to create.
    pub tag: ControlTag,
    /// Input type for `Input` tags: `text`, `password`, `checkbox`, or `radio`.
    pub input_type: Option<String>,
    /// Field semantics for building commands.
    pub kind: ControlKind,
    /// Shared field this widget edits.
    pub field: FieldId,
    /// Option this widget toggles within a checkbox or radio group.
    pub option: Option<OptionId>,
    /// Accessible label.
    pub label: String,
    /// Whether the field requires a value.
    pub required: bool,
    /// Current text value for text and editable combo controls.
    pub value: String,
    /// Maximum text length in Unicode scalar values.
    pub max_length: Option<u32>,
    /// Whether a checkbox or radio control is on.
    pub checked: bool,
    /// Push button caption.
    pub caption: String,
    /// Choices for list and combo controls.
    pub options: Option<Vec<ChoiceOption>>,
    /// Whether several list options may be selected.
    pub multiple: bool,
    /// Present choices as a visible list instead of a drop-down.
    pub list_box: bool,
    /// Indices into `options` that are selected.
    pub selected: Vec<u32>,
    /// Index of the first visible list option.
    pub top_index: u32,
    /// Counterclockwise content rotation in degrees from the source appearance.
    pub rotation: i32,
}

/// One Core annotation projected into logical device coordinates for a host to present.
///
/// Geometry inside `content` stays in page user space; `origin`, `size`, and
/// `transform` describe the annotation-local box the host draws into. Hosts build
/// their controls and visuals from the Core payload without interpreting PDF data;
/// retained source records and field definitions stay in the overlay.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct AnnotationEntry {
    /// Core identity the host echoes back in command targets.
    pub id: AnnotationId,
    /// Zero-based page containing the annotation.
    pub page: u32,
    /// Live Core payload.
    pub content: AnnotationKind,
    /// Live presentation style resolved from Core state and retained source data.
    pub style: ResolvedStyle,
    /// Host-owned action notification; links fall back to their destination.
    pub action: Option<AnnotationAction>,
    /// Live Unicode contents.
    pub text: String,
    /// Displayed subtype name.
    pub subtype: String,
    /// Why the host should show a placeholder instead of `annotation.content`.
    pub unsupported: Option<String>,
    /// Device-space interaction rectangles.
    pub bounds: Vec<[f64; 4]>,
    /// Page-space bottom-left corner of the `size` box; local coordinates are
    /// `[x - origin[0], origin[1] + size[1] - y]`.
    pub origin: [f64; 2],
    /// Local page-unit width and height.
    pub size: [f64; 2],
    /// Local top-left to device affine matrix.
    pub transform: [f64; 6],
    /// Preserve physical size under zoom.
    pub no_zoom: bool,
    /// Preserve orientation under rotation.
    pub no_rotate: bool,
    /// Permit host dragging.
    pub draggable: bool,
    /// Permit editing the content: the FreeText editor, or a widget's field value.
    pub editable: bool,
    /// Show this entry.
    pub visible: bool,
    /// Permit pointer interaction.
    pub interactive: bool,
    /// Document revision that last changed this annotation or its field.
    pub modified_revision: Revision,
    /// Decoded stamp name or note icon name; empty for other kinds.
    pub label: String,
    /// Validated `http`, `https`, or `mailto` link target.
    pub href: Option<String>,
    /// Whether a popup on this page names this annotation as its parent.
    pub has_popup: bool,
    /// Native control for widgets bound to a supported field.
    pub control: Option<WidgetControl>,
    /// Local-unit outlines for markup, ink, and geometric annotations.
    pub shapes: Vec<Shape>,
}
