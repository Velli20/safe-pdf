use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_lookup::ObjectLookupExt,
    object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    AnnotationDestination, AnnotationError, FileSpecification, Rendition, ThreeDView, helpers,
};

/// An annotation action.
#[allow(clippy::large_enum_variant)]
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
        /// The Windows launch dictionary.
        windows: Option<Dictionary>,
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
        /// The rendition dictionary.
        rendition: Option<Rendition>,
    },
    /// A transition action.
    Trans {
        /// The transition dictionary.
        transition: Option<Dictionary>,
        /// The duration.
        duration: Option<f32>,
    },
    /// A GoTo3DView action.
    GoTo3DView {
        /// The 3D view.
        view: Option<ThreeDView>,
    },
    /// A vendor or future action type.
    Unknown {
        /// The action subtype name.
        action_type: String,
    },
}

impl AnnotationAction {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        key: &'static str,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(action_dictionary) = dictionary.optional_dictionary(key, objects)? else {
            return Ok(None);
        };

        let action = match action_dictionary.required_str("S", objects)? {
            "GoTo" => Self::GoTo {
                destination: AnnotationDestination::from_object(
                    action_dictionary.get_or_err("D")?,
                    "D",
                    objects,
                )?,
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
                new_window: action_dictionary.optional_boolean("NewWindow", objects)?,
            },
            "URI" => Self::Uri {
                uri: action_dictionary.required_bytes_vec("URI", objects)?,
                is_map: action_dictionary.optional_boolean("IsMap", objects)?,
            },
            "Launch" => Self::Launch {
                file_specification: FileSpecification::from_dictionary(
                    action_dictionary,
                    "F",
                    objects,
                )?,
                windows: action_dictionary
                    .get("Win")
                    .map(|value| helpers::dictionary(value, objects))
                    .transpose()?,
            },
            "Named" => Self::Named {
                name: action_dictionary.required_bytes_vec("N", objects)?,
            },
            "SubmitForm" => Self::SubmitForm {
                file_specification: FileSpecification::from_dictionary(
                    action_dictionary,
                    "F",
                    objects,
                )?,
                fields: name_list(action_dictionary, "Fields", objects)?,
                flags: action_dictionary.optional_number::<i32>("Flags", objects)?,
            },
            "ResetForm" => Self::ResetForm {
                fields: name_list(action_dictionary, "Fields", objects)?,
                flags: action_dictionary.optional_number::<i32>("Flags", objects)?,
            },
            "ImportData" => Self::ImportData {
                file_specification: FileSpecification::from_object(
                    action_dictionary.get_or_err("F")?,
                    objects,
                )?,
            },
            "JavaScript" => Self::JavaScript {
                script: javascript_bytes(action_dictionary.get_or_err("JS")?, objects)?,
            },
            "SetOCGState" => Self::SetOCGState {
                state: name_list(action_dictionary, "State", objects)?.unwrap_or_default(),
                preserve_rb: action_dictionary.optional_boolean("PreserveRB", objects)?,
            },
            "Rendition" => Self::Rendition {
                operation: action_dictionary.optional_number::<i32>("OP", objects)?,
                rendition: Rendition::from_dictionary(action_dictionary, objects)?,
            },
            "Trans" => Self::Trans {
                transition: action_dictionary
                    .get("Trans")
                    .map(|value| helpers::dictionary(value, objects))
                    .transpose()?,
                duration: action_dictionary.optional_number::<f32>("D", objects)?,
            },
            "GoTo3DView" => Self::GoTo3DView {
                view: three_d_view(action_dictionary, "TA", objects)?,
            },
            other => Self::Unknown {
                action_type: other.to_owned(),
            },
        };

        Ok(Some(action))
    }
}

fn javascript_bytes(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Vec<u8>, AnnotationError> {
    let object = if let ObjectVariant::Reference(_) = value {
        objects.resolve_object(value)?
    } else {
        value
    };

    match object {
        ObjectVariant::HexString(bytes)
        | ObjectVariant::Name(bytes)
        | ObjectVariant::LiteralString(bytes) => Ok(bytes.clone()),
        ObjectVariant::Stream(stream) => Ok(stream.raw_data().to_vec()),
        _ => Err(ObjectError::TypeMismatch("Bytes", object.name()).into()),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{object_resolver::PassthroughResolver, stream::StreamObject};

    use super::*;

    struct TestResolver {
        objects: BTreeMap<usize, ObjectVariant>,
    }

    impl ObjectResolver for TestResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, ObjectError> {
            let ObjectVariant::Reference(object_number) = obj else {
                return Ok(obj);
            };

            self.objects
                .get(object_number)
                .ok_or(ObjectError::FailedResolveObjectReference {
                    obj_num: *object_number,
                })
        }
    }

    fn action_dictionary(js: ObjectVariant) -> Dictionary {
        Dictionary::new(BTreeMap::from([(
            "A".to_owned(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([
                ("S".to_owned(), ObjectVariant::Name(b"JavaScript".to_vec())),
                ("JS".to_owned(), js),
            ])))),
        )]))
    }

    fn stream_object(data: &[u8]) -> ObjectVariant {
        ObjectVariant::Stream(StreamObject::new(
            7,
            0,
            Box::new(Dictionary::new(BTreeMap::new())),
            data.to_vec(),
        ))
    }

    fn parse_action(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<AnnotationAction, AnnotationError> {
        AnnotationAction::from_dictionary(dictionary, "A", objects)?
            .ok_or(AnnotationError::MissingEntry { entry: "A" })
    }

    #[test]
    fn javascript_action_accepts_inline_string_script() {
        let dictionary = action_dictionary(ObjectVariant::LiteralString(b"app.alert(1);".to_vec()));

        let action = parse_action(&dictionary, &PassthroughResolver)
            .expect("JavaScript action should parse");

        match action {
            AnnotationAction::JavaScript { script } => {
                assert_eq!(script, b"app.alert(1);");
            }
            _ => panic!("expected JavaScript action"),
        }
    }

    #[test]
    fn javascript_action_accepts_stream_script() {
        let dictionary = action_dictionary(stream_object(b"this.dirty = false;"));

        let action = parse_action(&dictionary, &PassthroughResolver)
            .expect("JavaScript action should parse");

        match action {
            AnnotationAction::JavaScript { script } => {
                assert_eq!(script, b"this.dirty = false;");
            }
            _ => panic!("expected JavaScript action"),
        }
    }

    #[test]
    fn javascript_action_accepts_referenced_stream_script() {
        let dictionary = action_dictionary(ObjectVariant::Reference(7));
        let objects = TestResolver {
            objects: BTreeMap::from([(7, stream_object(b"var hidden = true;"))]),
        };

        let action = parse_action(&dictionary, &objects).expect("JavaScript action should parse");

        match action {
            AnnotationAction::JavaScript { script } => {
                assert_eq!(script, b"var hidden = true;");
            }
            _ => panic!("expected JavaScript action"),
        }
    }

    #[test]
    fn javascript_action_rejects_non_byte_script() {
        let dictionary = action_dictionary(ObjectVariant::Integer(1));
        let error = match parse_action(&dictionary, &PassthroughResolver) {
            Ok(_) => panic!("invalid JavaScript script should fail"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            AnnotationError::Object(ObjectError::TypeMismatch("Bytes", "Integer"))
        ));
    }
}
