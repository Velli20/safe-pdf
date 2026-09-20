//! Owned PDF metadata. These values retain source information without resolving resources.
//! Raw byte fields are never interpreted as UTF-8 merely to serialize them.
use crate::{metadata::AnnotationFlags, models::Quad};
use pdf_graphics::{DashPattern, color::Color, polyline::Polyline, rect::Rect};

bitflags::bitflags! {
    /// Widget form field flags parsed from the inherited `/Ff` entry.
    ///
    /// Unknown bits are preserved with `from_bits_retain` so future PDF flags
    /// round-trip without being dropped.
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct WidgetFieldFlags: i32 {
        /// The user may not change the value of the field.
        const READ_ONLY = 1 << 0;
        /// The field must have a value when the form is submitted.
        const REQUIRED = 1 << 1;
        /// The text field may contain multiple lines.
        const MULTILINE = 1 << 12;
        /// The text field is a password field whose value is not echoed.
        const PASSWORD = 1 << 13;
        /// A selected radio button may not be toggled off by user interaction.
        const NO_TOGGLE_TO_OFF = 1 << 14;
        /// The widget belongs to a radio button field.
        const RADIO_BUTTON = 1 << 15;
        /// The widget is a push button.
        const PUSH_BUTTON = 1 << 16;
        /// The choice field is a combo box rather than a listbox.
        const COMBO_BOX = 1 << 17;
        /// The combo box accepts a typed value besides its listed options.
        const EDIT = 1 << 18;
        /// The choice field permits more than one selected option.
        const MULTI_SELECT = 1 << 21;
        /// Radio buttons with the same on-state value change in unison.
        const RADIOS_IN_UNISON = 1 << 25;
    }
}

impl serde::Serialize for WidgetFieldFlags {
    /// Serializes the raw `/Ff` integer so unknown bits survive the wire format.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i32(self.bits())
    }
}

impl<'de> serde::Deserialize<'de> for WidgetFieldFlags {
    /// Deserializes the raw `/Ff` integer, retaining bits unknown to this version.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        i32::deserialize(deserializer).map(Self::from_bits_retain)
    }
}

/// Owned AnnotationBorder data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct AnnotationBorder {
    /// Horizontal corner radius.
    pub horizontal_radius: f64,
    /// Vertical corner radius.
    pub vertical_radius: f64,
    /// Border width.
    pub width: f64,
    /// Optional validated dash pattern; `None` is a solid border.
    pub dash_pattern: Option<DashPattern>,
}

/// Owned BorderStyle data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct BorderStyle {
    /// Border width.
    pub width: Option<f64>,
    /// Border style name.
    pub style: Option<BorderStyleName>,
    /// Optional dash pattern.
    pub dash_pattern: Option<Vec<f64>>,
}

/// Owned BorderStyleName data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum BorderStyleName {
    /// Solid line.
    Solid,
    /// Dashed line.
    Dashed,
    /// Beveled border.
    Beveled,
    /// Inset border.
    Inset,
    /// Underline border.
    Underline,
    /// A vendor or future border style.
    Unknown(Vec<u8>),
}

/// Owned BorderEffect data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct BorderEffect {
    /// The effect style.
    pub style: Option<BorderEffectStyle>,
    /// The intensity.
    pub intensity: Option<f64>,
}

/// Owned BorderEffectStyle data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum BorderEffectStyle {
    /// No border effect.
    None,
    /// A cloudy border effect.
    Cloudy,
    /// A vendor or future border effect.
    Unknown(Vec<u8>),
}

/// Owned CaretSymbolStyle data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum CaretSymbolStyle {
    /// Insert text marker.
    P,
    /// No marker.
    None,
    /// A vendor or future caret symbol style.
    Unknown(Vec<u8>),
}

/// Owned LineEndingStyle data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum LineEndingStyle {
    /// A butt end.
    Butt,
    /// A circular end.
    Circle,
    /// A diamond end.
    Diamond,
    /// An open arrow end.
    OpenArrow,
    /// A closed arrow end.
    ClosedArrow,
    /// No end marker.
    None,
    /// A square end.
    Square,
    /// A slashed end.
    Slash,
    /// A vendor or future line ending style.
    Unknown(Vec<u8>),
}

/// Owned LinkHighlightMode data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum LinkHighlightMode {
    /// Invert the annotation appearance.
    Invert,
    /// Outline the annotation.
    Outline,
    /// Push the annotation.
    Push,
    /// Toggle no highlight.
    None,
    /// Use the display rectangle.
    Toggle,
    /// A vendor or future mode.
    Unknown(Vec<u8>),
}

/// Owned AppearanceCharacteristics data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct AppearanceCharacteristics {
    /// The rotation in degrees.
    pub rotation: Option<i32>,
    /// The border color.
    pub border_color: Option<Color>,
    /// The background color.
    pub background_color: Option<Color>,
    /// The normal caption.
    pub normal_caption: Option<Vec<u8>>,
    /// The rollover caption.
    pub rollover_caption: Option<Vec<u8>>,
    /// The alternate caption.
    pub alternate_caption: Option<Vec<u8>>,
}

/// Owned AppearanceDictionary data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct AppearanceDictionary {
    /// Normal-state names; no drawing resources are retained.
    pub normal_states: Vec<Vec<u8>>,
}

/// Owned FileSpecification data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum FileSpecification {
    /// A file specification dictionary.
    Dictionary(Box<FileSpecificationDictionary>),
    /// A direct path string.
    Path(Vec<u8>),
}

/// Owned FileSpecificationDictionary data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct FileSpecificationDictionary {
    /// The file system name.
    pub file_system: Option<Vec<u8>>,
    /// The platform-independent file name.
    pub file_name: Option<Vec<u8>>,
    /// The Unicode file name.
    pub unicode_file_name: Option<Vec<u8>>,
    /// The macOS file name.
    pub mac_file_name: Option<Vec<u8>>,
    /// The DOS file name.
    pub dos_file_name: Option<Vec<u8>>,
    /// The Unix file name.
    pub unix_file_name: Option<Vec<u8>>,
    /// Whether the file is volatile.
    pub volatile: Option<bool>,
}

/// Owned AnnotationDestination data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum AnnotationDestination {
    /// A named destination.
    Named {
        /// Raw destination name bytes.
        name: Vec<u8>,
    },
    /// An explicit destination array.
    Explicit(Box<ExplicitDestination>),
}

/// Owned DestinationTarget data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum DestinationTarget {
    /// A direct page dictionary; the host resolves it in the document model.
    Dictionary,
    /// A page reference into the containing source PDF.
    Reference {
        /// PDF object number, distinct from Core annotation and field identities.
        #[serde(with = "crate::wire")]
        #[cfg_attr(feature = "typescript", ts(type = "string"))]
        number: u64,
        /// PDF generation number.
        #[serde(with = "crate::wire")]
        #[cfg_attr(feature = "typescript", ts(type = "string"))]
        generation: u64,
    },
}

/// Owned ExplicitDestination data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum ExplicitDestination {
    /// `/XYZ` destination.
    Xyz {
        /// The page target.
        page: DestinationTarget,
        /// Left coordinate.
        left: Option<f64>,
        /// Top coordinate.
        top: Option<f64>,
        /// Zoom factor.
        zoom: Option<f64>,
    },
    /// `/Fit` destination.
    Fit {
        /// The page target.
        page: DestinationTarget,
    },
    /// `/FitH` destination.
    FitH {
        /// The page target.
        page: DestinationTarget,
        /// Top coordinate.
        top: Option<f64>,
    },
    /// `/FitV` destination.
    FitV {
        /// The page target.
        page: DestinationTarget,
        /// Left coordinate.
        left: Option<f64>,
    },
    /// `/FitR` destination.
    FitR {
        /// The page target.
        page: DestinationTarget,
        /// Left coordinate.
        left: f64,
        /// Bottom coordinate.
        bottom: f64,
        /// Right coordinate.
        right: f64,
        /// Top coordinate.
        top: f64,
    },
    /// `/FitB` destination.
    FitB {
        /// The page target.
        page: DestinationTarget,
    },
    /// `/FitBH` destination.
    FitBH {
        /// The page target.
        page: DestinationTarget,
        /// Top coordinate.
        top: Option<f64>,
    },
    /// `/FitBV` destination.
    FitBV {
        /// The page target.
        page: DestinationTarget,
        /// Left coordinate.
        left: Option<f64>,
    },
}

/// Owned AnnotationAction data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum AnnotationAction {
    /// A go-to action.
    GoTo {
        /// The destination.
        destination: AnnotationDestination,
    },
    /// A remote go-to action.
    GoToRemote {
        /// The file specification.
        file_specification: FileSpecification,
        /// The destination.
        destination: Option<AnnotationDestination>,
        /// Whether to open in a new window.
        new_window: Option<bool>,
    },
    /// A URI action.
    Uri {
        /// The URI bytes.
        uri: Vec<u8>,
        /// Whether to treat the URI as a map request.
        is_map: Option<bool>,
    },
    /// A launch action.
    Launch {
        /// The file specification.
        file_specification: Option<FileSpecification>,
    },
    /// A named action.
    Named {
        /// The action name.
        name: Vec<u8>,
    },
    /// A submit-form action.
    SubmitForm {
        /// The file specification.
        file_specification: Option<FileSpecification>,
        /// The field names.
        fields: Option<Vec<Vec<u8>>>,
        /// The submit flags.
        flags: Option<i32>,
    },
    /// A reset-form action.
    ResetForm {
        /// The field names.
        fields: Option<Vec<Vec<u8>>>,
        /// The reset flags.
        flags: Option<i32>,
    },
    /// An import-data action.
    ImportData {
        /// The file specification.
        file_specification: FileSpecification,
    },
    /// A JavaScript action.
    JavaScript {
        /// The script bytes.
        script: Vec<u8>,
    },
    /// A SetOCGState action.
    SetOCGState {
        /// The OCG state names.
        state: Vec<Vec<u8>>,
        /// Whether to preserve the radiobutton state.
        preserve_rb: Option<bool>,
    },
    /// A rendition action.
    Rendition {
        /// The operation code.
        operation: Option<i32>,
    },
    /// A transition action.
    Trans {
        /// The duration.
        duration: Option<f64>,
    },
    /// A GoTo3DView action; the view dictionary is not retained.
    GoTo3DView,
    /// A vendor or future action type.
    Unknown {
        /// The action subtype name.
        action_type: Vec<u8>,
    },
}

impl AnnotationAction {
    /// Trimmed URI of a `Uri` action when it is an absolute `http`, `https`, or
    /// `mailto` target a host may navigate to; other schemes and actions yield `None`.
    pub fn href(&self) -> Option<String> {
        let Self::Uri { uri, .. } = self else {
            return None;
        };
        let uri = String::from_utf8_lossy(uri).trim().to_owned();
        let lower = uri.to_ascii_lowercase();
        ["http://", "https://", "mailto:"]
            .iter()
            .any(|scheme| lower.starts_with(scheme))
            .then_some(uri)
    }
}

/// Owned InkList data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct InkList {
    /// The parsed stroke lists.
    pub strokes: Vec<Polyline<f64>>,
}

/// Owned CaretAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct CaretAnnotation {
    /// The difference rectangle.
    pub difference_rect: Option<[f64; 4]>,
    /// The caret style.
    pub style: Option<CaretSymbolStyle>,
}

/// Owned CircleAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct CircleAnnotation {
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The interior color.
    pub interior_color: Option<Color>,
    /// The border effect.
    pub border_effect: Option<BorderEffect>,
    /// The difference rectangle.
    pub difference_rect: Option<[f64; 4]>,
}

/// Owned FreeTextAlignment data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum FreeTextAlignment {
    /// Align lines with the left edge of the padded content area.
    Left,
    /// Center lines within the padded content area.
    Center,
    /// Align lines with the right edge of the padded content area.
    Right,
}

/// Owned FreeTextAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct FreeTextAnnotation {
    /// The default appearance string.
    pub default_appearance: Option<Vec<u8>>,
    /// The quadding mode.
    pub quadding: Option<i32>,
    /// Rich text contents.
    pub rich_contents: Option<Vec<u8>>,
    /// The default style string.
    pub default_style: Option<Vec<u8>>,
    /// The open callout line, from its starting point to its attachment point.
    pub callout_line: Option<Polyline<f64>>,
    /// The border effect.
    pub border_effect: Option<BorderEffect>,
    /// The difference rectangle.
    pub difference_rect: Option<[f64; 4]>,
    /// The intent.
    pub intent: Option<Vec<u8>>,
}

/// Owned HighlightAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct HighlightAnnotation {
    /// The quadrilaterals in perimeter order.
    pub quad_points: Vec<Quad>,
    /// The annotation color.
    pub color: Option<Color>,
    /// The constant opacity.
    pub constant_opacity: Option<f64>,
}

/// Owned InkAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct InkAnnotation {
    /// The required ink list.
    pub ink_list: InkList,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The interior color.
    pub interior_color: Option<Color>,
}

/// Owned LineAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct LineAnnotation {
    /// The required line endpoints.
    pub line: [f64; 4],
    /// The line ending styles.
    pub line_endings: Option<[LineEndingStyle; 2]>,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The interior color.
    pub interior_color: Option<Color>,
    /// The leader line length.
    pub leader_line_length: Option<f64>,
    /// The leader line extension length.
    pub leader_line_extension: Option<f64>,
    /// Whether the caption is positioned at the line's end.
    pub caption: Option<bool>,
    /// The intent.
    pub intent: Option<Vec<u8>>,
}

/// Owned LinkAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct LinkAnnotation {
    /// The link highlight mode.
    pub highlight_mode: Option<LinkHighlightMode>,
    /// The annotation destination.
    pub destination: Option<AnnotationDestination>,
    /// The annotation action.
    pub action: Option<AnnotationAction>,
    /// The quadrilaterals in perimeter order.
    pub quad_points: Option<Vec<Quad>>,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The border effect.
    pub border_effect: Option<BorderEffect>,
}

/// Owned PolygonAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct PolygonAnnotation {
    /// The required polygon vertices.
    pub vertices: Polyline<f64>,
    /// The line ending styles.
    pub line_endings: Option<[LineEndingStyle; 2]>,
    /// The interior color.
    pub interior_color: Option<Color>,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The intent.
    pub intent: Option<Vec<u8>>,
}

/// Owned PolyLineAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct PolyLineAnnotation {
    /// The required polyline vertices.
    pub vertices: Polyline<f64>,
    /// The line ending styles.
    pub line_endings: Option<[LineEndingStyle; 2]>,
    /// The interior color.
    pub interior_color: Option<Color>,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The intent.
    pub intent: Option<Vec<u8>>,
}

/// Owned PopupAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct PopupAnnotation {
    /// The required parent annotation reference.
    #[serde(with = "crate::wire::optional")]
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub parent: Option<u64>,
    /// Whether the popup is open.
    pub open: Option<bool>,
}

/// Owned SquareAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct SquareAnnotation {
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The interior color.
    pub interior_color: Option<Color>,
    /// The border effect.
    pub border_effect: Option<BorderEffect>,
    /// The difference rectangle.
    pub difference_rect: Option<[f64; 4]>,
}

/// Owned SquigglyAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct SquigglyAnnotation {
    /// The quadrilaterals in perimeter order.
    pub quad_points: Vec<Quad>,
    /// The annotation color.
    pub color: Option<Color>,
    /// The constant opacity.
    pub constant_opacity: Option<f64>,
}

/// Owned StampAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct StampAnnotation {
    /// The stamp name.
    pub name: Option<Vec<u8>>,
}

/// Owned StrikeOutAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct StrikeOutAnnotation {
    /// The quadrilaterals in perimeter order.
    pub quad_points: Vec<Quad>,
    /// The annotation color.
    pub color: Option<Color>,
    /// The constant opacity.
    pub constant_opacity: Option<f64>,
}

/// Owned TextAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct TextAnnotation {
    /// Whether the note popup starts open.
    pub open: Option<bool>,
    /// The note icon name.
    pub name: Option<Vec<u8>>,
    /// The annotation state.
    pub state: Option<Vec<u8>>,
    /// The annotation state model.
    pub state_model: Option<Vec<u8>>,
    /// The intent name.
    pub intent: Option<Vec<u8>>,
}

/// Owned UnderlineAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct UnderlineAnnotation {
    /// The quadrilaterals in perimeter order.
    pub quad_points: Vec<Quad>,
    /// The annotation color.
    pub color: Option<Color>,
    /// The constant opacity.
    pub constant_opacity: Option<f64>,
}

/// Owned WidgetChoiceOption data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct WidgetChoiceOption {
    /// Value written to the field when this option is selected.
    pub export_value: Vec<u8>,
    /// Value presented to the user for this option.
    pub display_value: Vec<u8>,
}

/// Owned WidgetFieldValue data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum WidgetFieldValue {
    /// A string-like value represented as raw PDF bytes.
    Bytes(Vec<u8>),
    /// A dictionary value, such as a signature field value; contents are not retained.
    Dictionary,
    /// An array of widget field values.
    Array(Vec<WidgetFieldValue>),
    /// An explicit PDF null value.
    Null,
}

/// Owned WidgetAnnotation data retained from the source PDF.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct WidgetAnnotation {
    /// Indirect object number of the terminal field that owns this widget.
    ///
    /// Widgets belonging to the same logical field share this identifier even
    /// when other field properties are inherited from higher ancestors.
    #[serde(with = "crate::wire::optional")]
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub field_id: Option<u64>,
    /// The form field type.
    pub field_type: Option<Vec<u8>>,
    /// The field name.
    pub field_name: Option<Vec<u8>>,
    /// The alternate field name.
    pub alternate_name: Option<Vec<u8>>,
    /// The mapping name.
    pub mapping_name: Option<Vec<u8>>,
    /// The field flags.
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub field_flags: Option<WidgetFieldFlags>,
    /// The value.
    pub value: Option<WidgetFieldValue>,
    /// The default value.
    pub default_value: Option<WidgetFieldValue>,
    /// The default appearance string.
    pub default_appearance: Option<Vec<u8>>,
    /// The quadding mode.
    pub quadding: Option<i32>,
    /// Maximum number of text characters.
    #[serde(with = "crate::wire::optional")]
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub max_length: Option<u64>,
    /// Options available to a choice field.
    pub options: Option<Vec<WidgetChoiceOption>>,
    /// Explicitly selected choice option indices from `/I`.
    #[serde(with = "crate::wire::optional_vec")]
    #[cfg_attr(feature = "typescript", ts(type = "Array<string> | null"))]
    pub selected_indices: Option<Vec<u64>>,
    /// Index of the first visible listbox option from `/TI`.
    #[serde(with = "crate::wire::optional")]
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub top_index: Option<u64>,
    /// The appearance characteristics.
    pub appearance_characteristics: Option<AppearanceCharacteristics>,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The action dictionary.
    pub action: Option<AnnotationAction>,
}

/// Native PDF annotation subtype data.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum NativeAnnotation {
    /// A text annotation.
    Text(Box<TextAnnotation>),
    /// A link annotation.
    Link(Box<LinkAnnotation>),
    /// A free text annotation.
    FreeText(Box<FreeTextAnnotation>),
    /// A line annotation.
    Line(Box<LineAnnotation>),
    /// A square annotation.
    Square(Box<SquareAnnotation>),
    /// A circle annotation.
    Circle(Box<CircleAnnotation>),
    /// A polygon annotation.
    Polygon(Box<PolygonAnnotation>),
    /// A polyline annotation.
    PolyLine(Box<PolyLineAnnotation>),
    /// A highlight annotation.
    Highlight(Box<HighlightAnnotation>),
    /// An underline annotation.
    Underline(Box<UnderlineAnnotation>),
    /// A squiggly annotation.
    Squiggly(Box<SquigglyAnnotation>),
    /// A strikeout annotation.
    StrikeOut(Box<StrikeOutAnnotation>),
    /// A stamp annotation.
    Stamp(Box<StampAnnotation>),
    /// A caret annotation.
    Caret(Box<CaretAnnotation>),
    /// An ink annotation.
    Ink(Box<InkAnnotation>),
    /// A popup annotation.
    Popup(Box<PopupAnnotation>),
    /// A widget annotation.
    Widget(Box<WidgetAnnotation>),
    /// A vendor or future annotation subtype not covered by PDF 1.7 base types.
    Unknown {
        /// Unrecognized PDF subtype bytes.
        subtype: Vec<u8>,
    },
}

/// Native PDF annotation data retained from a page object.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct SourceAnnotation {
    /// Original page-local runtime identity; never used as a Core identity.
    #[serde(with = "crate::wire")]
    #[cfg_attr(feature = "typescript", ts(type = "string"))]
    pub id: u64,
    /// Original indirect object number, used to resolve annotation relationships.
    #[serde(with = "crate::wire::optional")]
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub object_number: Option<u64>,
    /// Opacity applied by native presentation.
    pub opacity: f64,
    /// The annotation subtype name.
    pub subtype: Vec<u8>,
    /// The optional annotation rectangle from `/Rect`, in PDF page user space
    /// where `bottom` is the lower `y` edge. Edges keep their source values and
    /// order, so inverted rectangles are preserved rather than normalized.
    pub rect: Option<Rect<f64>>,
    /// The optional annotation contents.
    pub contents: Option<Vec<u8>>,
    /// The optional annotation name from `/NM`.
    pub name: Option<Vec<u8>>,
    /// The optional annotation flags from `/F`.
    #[cfg_attr(feature = "typescript", ts(type = "number | null"))]
    pub flags: Option<AnnotationFlags>,
    /// The optional appearance dictionary from `/AP`.
    pub appearance: Option<AppearanceDictionary>,
    /// The optional appearance state from `/AS`.
    pub appearance_state: Option<Vec<u8>>,
    /// The optional border array from `/Border`.
    pub border: Option<AnnotationBorder>,
    /// The optional annotation color from `/C`, converted from the source
    /// device color space to sRGB. An empty `/C` array is retained as a fully
    /// transparent color (alpha `0.0`), distinct from an absent entry.
    pub color: Option<Color>,
    /// The optional structure parent index from `/StructParent`.
    #[serde(with = "crate::wire::optional")]
    #[cfg_attr(feature = "typescript", ts(type = "string | null"))]
    pub struct_parent: Option<u64>,
    /// The parsed subtype-specific payload.
    pub kind: NativeAnnotation,
}
