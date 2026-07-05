use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{
    AnnotationDestination, AnnotationError, FileSpecification, Rendition, ThreeDView, helpers,
};

/// An annotation action.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationAction {
    /// A go-to action.
    GoTo {
        /// The destination.
        destination: AnnotationDestination,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A remote go-to action.
    GoToRemote {
        /// The file specification.
        file_specification: FileSpecification,
        /// The destination.
        destination: Option<AnnotationDestination>,
        /// Whether to open in a new window.
        new_window: Option<bool>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A URI action.
    Uri {
        /// The URI bytes.
        uri: Vec<u8>,
        /// Whether to treat the URI as a map request.
        is_map: Option<bool>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A launch action.
    Launch {
        /// The file specification.
        file_specification: Option<FileSpecification>,
        /// The Windows launch dictionary.
        windows: Option<Dictionary>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A named action.
    Named {
        /// The action name.
        name: Vec<u8>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A submit-form action.
    SubmitForm {
        /// The file specification.
        file_specification: Option<FileSpecification>,
        /// The field names.
        fields: Option<Vec<Vec<u8>>>,
        /// The submit flags.
        flags: Option<i32>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A reset-form action.
    ResetForm {
        /// The field names.
        fields: Option<Vec<Vec<u8>>>,
        /// The reset flags.
        flags: Option<i32>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// An import-data action.
    ImportData {
        /// The file specification.
        file_specification: FileSpecification,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A JavaScript action.
    JavaScript {
        /// The script bytes.
        script: Vec<u8>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A SetOCGState action.
    SetOCGState {
        /// The OCG state names.
        state: Vec<Vec<u8>>,
        /// Whether to preserve the radiobutton state.
        preserve_rb: Option<bool>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A rendition action.
    Rendition {
        /// The operation code.
        operation: Option<i32>,
        /// The rendition dictionary.
        rendition: Option<Rendition>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A transition action.
    Trans {
        /// The transition dictionary.
        transition: Option<Dictionary>,
        /// The duration.
        duration: Option<f32>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A GoTo3DView action.
    GoTo3DView {
        /// The 3D view.
        view: Option<ThreeDView>,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
    /// A vendor or future action type.
    Unknown {
        /// The action subtype name.
        action_type: String,
        /// The original action dictionary.
        dictionary: Dictionary,
    },
}

impl AnnotationAction {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        key: &'static str,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(value) = dictionary.get(key) else {
            return Ok(None);
        };

        let action_dictionary = value.try_dictionary(objects)?.clone();
        let action_type = action_dictionary
            .get_or_err("S")?
            .try_str(objects)?
            .into_owned();

        let action = match action_type.as_ref() {
            "GoTo" => Self::GoTo {
                destination: AnnotationDestination::from_object(
                    action_dictionary.get_or_err("D")?,
                    "D",
                    objects,
                )?,
                dictionary: action_dictionary,
            },
            "GoToR" => Self::GoToRemote {
                file_specification: FileSpecification::from_object(
                    action_dictionary.get_or_err("F")?,
                    objects,
                )?,
                destination: action_dictionary
                    .get("D")
                    .map(|value| AnnotationDestination::from_object(value, "D", objects))
                    .transpose()?,
                new_window: action_dictionary
                    .get("NewWindow")
                    .map(|value| value.try_boolean(objects))
                    .transpose()?,
                dictionary: action_dictionary,
            },
            "URI" => Self::Uri {
                uri: action_dictionary
                    .get_or_err("URI")?
                    .try_bytes_vec(objects)?,
                is_map: action_dictionary
                    .get("IsMap")
                    .map(|value| value.try_boolean(objects))
                    .transpose()?,
                dictionary: action_dictionary,
            },
            "Launch" => Self::Launch {
                file_specification: FileSpecification::from_dictionary(
                    &action_dictionary,
                    "F",
                    objects,
                )?,
                windows: action_dictionary
                    .get("Win")
                    .map(|value| helpers::dictionary(value, objects))
                    .transpose()?,
                dictionary: action_dictionary,
            },
            "Named" => Self::Named {
                name: action_dictionary.get_or_err("N")?.try_bytes_vec(objects)?,
                dictionary: action_dictionary,
            },
            "SubmitForm" => Self::SubmitForm {
                file_specification: FileSpecification::from_dictionary(
                    &action_dictionary,
                    "F",
                    objects,
                )?,
                fields: name_list(&action_dictionary, "Fields", objects)?,
                flags: action_dictionary
                    .get("Flags")
                    .map(|value| value.try_number::<i32>(objects))
                    .transpose()?,
                dictionary: action_dictionary,
            },
            "ResetForm" => Self::ResetForm {
                fields: name_list(&action_dictionary, "Fields", objects)?,
                flags: action_dictionary
                    .get("Flags")
                    .map(|value| value.try_number::<i32>(objects))
                    .transpose()?,
                dictionary: action_dictionary,
            },
            "ImportData" => Self::ImportData {
                file_specification: FileSpecification::from_object(
                    action_dictionary.get_or_err("F")?,
                    objects,
                )?,
                dictionary: action_dictionary,
            },
            "JavaScript" => Self::JavaScript {
                script: action_dictionary.get_or_err("JS")?.try_bytes_vec(objects)?,
                dictionary: action_dictionary,
            },
            "SetOCGState" => Self::SetOCGState {
                state: name_list(&action_dictionary, "State", objects)?.unwrap_or_default(),
                preserve_rb: action_dictionary
                    .get("PreserveRB")
                    .map(|value| value.try_boolean(objects))
                    .transpose()?,
                dictionary: action_dictionary,
            },
            "Rendition" => Self::Rendition {
                operation: action_dictionary
                    .get("OP")
                    .map(|value| value.try_number::<i32>(objects))
                    .transpose()?,
                rendition: Rendition::from_dictionary(&action_dictionary, objects)?,
                dictionary: action_dictionary,
            },
            "Trans" => Self::Trans {
                transition: action_dictionary
                    .get("Trans")
                    .map(|value| helpers::dictionary(value, objects))
                    .transpose()?,
                duration: action_dictionary
                    .get("D")
                    .map(|value| value.try_number::<f32>(objects))
                    .transpose()?,
                dictionary: action_dictionary,
            },
            "GoTo3DView" => Self::GoTo3DView {
                view: three_d_view(&action_dictionary, "TA", objects)?,
                dictionary: action_dictionary,
            },
            other => Self::Unknown {
                action_type: other.to_owned(),
                dictionary: action_dictionary,
            },
        };

        Ok(Some(action))
    }
}

pub(crate) fn name_list(
    dictionary: &Dictionary,
    key: &'static str,
    objects: &dyn ObjectResolver,
) -> Result<Option<Vec<Vec<u8>>>, AnnotationError> {
    let Some(value) = dictionary.get(key) else {
        return Ok(None);
    };

    let items = value.try_array(objects)?;
    let mut names = Vec::with_capacity(items.len());
    for item in items {
        names.push(item.try_bytes(objects)?.to_vec());
    }

    Ok(Some(names))
}

pub(crate) fn three_d_view(
    dictionary: &Dictionary,
    key: &'static str,
    objects: &dyn ObjectResolver,
) -> Result<Option<ThreeDView>, AnnotationError> {
    dictionary
        .get(key)
        .map(|value| {
            Ok(ThreeDView {
                dictionary: value.try_dictionary(objects)?.clone(),
            })
        })
        .transpose()
}
