//! Action, destination, and file-specification decoding; nothing is executed.
use crate::error::SourceDecodeError;
use crate::ocg_state::{OcgStateEntry, OcgTarget};
use crate::pdf_data::{
    AnnotationAction, AnnotationDestination, DestinationTarget, ExplicitDestination,
    FileSpecification, FileSpecificationDictionary,
};
use crate::source_decode::DecodeResult;
use pdf_object_reader::{
    dictionary::Dictionary, object_error::ObjectError, object_lookup::ObjectLookupExt,
    object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

impl AnnotationAction {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        key: &'static [u8],
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Option<Self>> {
        let Some(action_dictionary) = dictionary.optional_dictionary(key, objects)? else {
            return Ok(None);
        };

        let action = match action_dictionary.required_bytes(b"S", objects)? {
            b"GoTo" => Self::GoTo {
                destination: AnnotationDestination::from_object(
                    action_dictionary.get_or_err(b"D")?,
                    b"D",
                    objects,
                )?,
            },
            b"GoToR" => Self::GoToRemote {
                file_specification: FileSpecification::from_object(
                    action_dictionary.get_or_err(b"F")?,
                    objects,
                )?,
                destination: action_dictionary
                    .get(b"D")
                    .map(|value| AnnotationDestination::from_object(value, b"D", objects))
                    .transpose()?,
                new_window: action_dictionary.optional_boolean(b"NewWindow", objects)?,
            },
            b"URI" => Self::Uri {
                uri: action_dictionary.required_bytes_vec(b"URI", objects)?,
                is_map: action_dictionary.optional_boolean(b"IsMap", objects)?,
            },
            b"Launch" => Self::Launch {
                file_specification: FileSpecification::from_dictionary(
                    action_dictionary,
                    b"F",
                    objects,
                )?,
            },
            b"Named" => Self::Named {
                name: Vec::from(action_dictionary.required_bytes(b"N", objects)?),
            },
            b"SubmitForm" => Self::SubmitForm {
                file_specification: FileSpecification::from_dictionary(
                    action_dictionary,
                    b"F",
                    objects,
                )?,
                fields: name_list(action_dictionary, b"Fields", objects)?,
                flags: action_dictionary.optional_number::<i32>(b"Flags", objects)?,
            },
            b"ResetForm" => Self::ResetForm {
                fields: name_list(action_dictionary, b"Fields", objects)?,
                flags: action_dictionary.optional_number::<i32>(b"Flags", objects)?,
            },
            b"ImportData" => Self::ImportData {
                file_specification: FileSpecification::from_object(
                    action_dictionary.get_or_err(b"F")?,
                    objects,
                )?,
            },
            b"JavaScript" => Self::JavaScript {
                script: action_dictionary
                    .get_or_err(b"JS")?
                    .try_bytes_vec(objects)?,
            },
            b"SetOCGState" => Self::SetOCGState {
                state: ocg_state(action_dictionary, objects)?,
                preserve_rb: action_dictionary.optional_boolean(b"PreserveRB", objects)?,
            },
            b"Rendition" => Self::Rendition {
                operation: action_dictionary.optional_number::<i32>(b"OP", objects)?,
            },
            b"Trans" => Self::Trans {
                duration: action_dictionary.optional_number::<f64>(b"D", objects)?,
            },
            b"GoTo3DView" => Self::GoTo3DView,
            other => Self::Unknown {
                action_type: Vec::from(other),
            },
        };

        Ok(Some(action))
    }
}

impl AnnotationDestination {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        key: &'static [u8],
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Option<Self>> {
        dictionary
            .get(key)
            .map(|value| Self::from_object(value, key, objects))
            .transpose()
    }

    fn from_object(
        value: &ObjectVariant,
        entry: &'static [u8],
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        Ok(match value {
            ObjectVariant::String(value) => Self::Named {
                name: value.as_bytes().to_vec(),
            },
            ObjectVariant::Reference(_)
            | ObjectVariant::Dictionary(_)
            | ObjectVariant::Stream(_)
            | ObjectVariant::Array(_) => {
                Self::Explicit(Box::new(explicit_destination(value, entry, objects)?))
            }
            other => return Err(ObjectError::TypeMismatch("Destination", other.name()).into()),
        })
    }
}

fn explicit_destination(
    value: &ObjectVariant,
    entry: &'static [u8],
    objects: &dyn ObjectResolver,
) -> DecodeResult<ExplicitDestination> {
    let items = value.try_array(objects)?;

    let Some(page_item) = items.first() else {
        return Err(SourceDecodeError::InvalidEntry {
            entry,
            reason: "destination array is missing the page target".to_owned(),
        });
    };
    let Some(mode_item) = items.get(1) else {
        return Err(SourceDecodeError::InvalidEntry {
            entry,
            reason: "destination array is missing the destination type".to_owned(),
        });
    };

    let page = destination_target(page_item, objects)?;

    match mode_item.try_bytes(objects)? {
        b"XYZ" => Ok(ExplicitDestination::Xyz {
            page,
            left: items.optional_number(2, objects)?,
            top: items.optional_number(3, objects)?,
            zoom: items.optional_number(4, objects)?,
        }),
        b"Fit" => Ok(ExplicitDestination::Fit { page }),
        b"FitH" => Ok(ExplicitDestination::FitH {
            page,
            top: items.optional_number(2, objects)?,
        }),
        b"FitV" => Ok(ExplicitDestination::FitV {
            page,
            left: items.optional_number(2, objects)?,
        }),
        b"FitR" => Ok(ExplicitDestination::FitR {
            page,
            left: items.required_number(2, objects)?,
            bottom: items.required_number(3, objects)?,
            right: items.required_number(4, objects)?,
            top: items.required_number(5, objects)?,
        }),
        b"FitB" => Ok(ExplicitDestination::FitB { page }),
        b"FitBH" => Ok(ExplicitDestination::FitBH {
            page,
            top: items.optional_number(2, objects)?,
        }),
        b"FitBV" => Ok(ExplicitDestination::FitBV {
            page,
            left: items.optional_number(2, objects)?,
        }),
        other => Err(SourceDecodeError::InvalidEntry {
            entry,
            reason: format!("unsupported destination type '{other:?}'"),
        }),
    }
}

fn destination_target(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> DecodeResult<DestinationTarget> {
    Ok(match value {
        ObjectVariant::Reference(id) => DestinationTarget::Reference {
            number: u64::try_from(id.number).map_err(|_| SourceDecodeError::ResourceLimit)?,
            generation: u64::try_from(id.generation)
                .map_err(|_| SourceDecodeError::ResourceLimit)?,
        },
        _ => {
            value.try_dictionary(objects)?;
            DestinationTarget::Dictionary
        }
    })
}

impl FileSpecification {
    fn from_dictionary(
        dictionary: &Dictionary,
        key: &'static [u8],
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Option<Self>> {
        dictionary
            .get(key)
            .map(|value| Self::from_object(value, objects))
            .transpose()
    }

    fn from_object(value: &ObjectVariant, objects: &dyn ObjectResolver) -> DecodeResult<Self> {
        let Ok(dictionary) = value.try_dictionary(objects) else {
            return Ok(Self::Path(value.try_bytes(objects)?.to_vec()));
        };

        let file_system = dictionary.optional_bytes(b"FS", objects)?.map(Vec::from);
        let file_name = dictionary.optional_bytes_vec(b"F", objects)?;
        let unicode_file_name = dictionary.optional_bytes_vec(b"UF", objects)?;
        let mac_file_name = dictionary.optional_bytes_vec(b"Mac", objects)?;
        let dos_file_name = dictionary.optional_bytes_vec(b"DOS", objects)?;
        let unix_file_name = dictionary.optional_bytes_vec(b"Unix", objects)?;
        let volatile = dictionary.optional_boolean(b"V", objects)?;

        Ok(Self::Dictionary(Box::new(FileSpecificationDictionary {
            file_system,
            file_name,
            unicode_file_name,
            mac_file_name,
            dos_file_name,
            unix_file_name,
            volatile,
        })))
    }
}

/// Decodes the `/State` array of a `SetOCGState` action.
///
/// Elements alternate between operation names and the group targets they govern,
/// so the sequence is retained in source order rather than collected as names.
fn ocg_state(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> DecodeResult<Vec<OcgStateEntry>> {
    const ENTRY: &[u8] = b"State";

    let Some(value) = dictionary.get(ENTRY) else {
        return Err(SourceDecodeError::MissingEntry { entry: ENTRY });
    };

    let items = value.try_array(objects)?;
    let mut state = Vec::with_capacity(items.len());
    let mut seen_operation = false;
    let mut pending_operation = false;

    for item in items {
        if item.is_name() {
            if pending_operation {
                return Err(SourceDecodeError::InvalidEntry {
                    entry: ENTRY,
                    reason: "optional content operation has no group target".to_owned(),
                });
            }
            state.push(OcgStateEntry::try_from(item.try_bytes(objects)?)?);
            seen_operation = true;
            pending_operation = true;
            continue;
        }

        if !seen_operation {
            return Err(SourceDecodeError::InvalidEntry {
                entry: ENTRY,
                reason: "group target precedes any optional content operation".to_owned(),
            });
        }
        state.push(OcgStateEntry::Group(ocg_target(item, objects)?));
        pending_operation = false;
    }

    if pending_operation {
        return Err(SourceDecodeError::InvalidEntry {
            entry: ENTRY,
            reason: "optional content operation has no group target".to_owned(),
        });
    }

    Ok(state)
}

/// Decodes one optional content group target, keeping references unresolved.
fn ocg_target(value: &ObjectVariant, objects: &dyn ObjectResolver) -> DecodeResult<OcgTarget> {
    if let ObjectVariant::Reference(id) = value {
        return Ok(OcgTarget::Reference {
            number: u64::try_from(id.number).map_err(|_| SourceDecodeError::ResourceLimit)?,
            generation: u64::try_from(id.generation)
                .map_err(|_| SourceDecodeError::ResourceLimit)?,
        });
    }

    let group = value.try_dictionary(objects)?;
    let group_type = group.required_bytes(b"Type", objects)?;
    if group_type != b"OCG" {
        return Err(SourceDecodeError::InvalidEntry {
            entry: b"State",
            reason: format!("optional content group has type '{group_type:?}'"),
        });
    }

    Ok(OcgTarget::Dictionary {
        name: group.required_bytes_vec(b"Name", objects)?,
    })
}

fn name_list(
    dictionary: &Dictionary,
    key: &'static [u8],
    objects: &dyn ObjectResolver,
) -> DecodeResult<Option<Vec<Vec<u8>>>> {
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
