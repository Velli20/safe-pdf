//! Action, destination, and file-specification decoding; nothing is executed.
use crate::error::SourceDecodeError;
use crate::pdf_data::{
    AnnotationAction, AnnotationDestination, DestinationTarget, ExplicitDestination,
    FileSpecification, FileSpecificationDictionary,
};
use pdf_object_reader::pdf_name::PdfName;
use pdf_object_reader::{
    ArrayContext, FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
    object_kind::ObjectKind, object_variant::ObjectVariant, pdf_array::PdfArray,
};
use pdf_optional_content::{OcgStateEntry, OcgTarget};

/// Reads an action dictionary, such as a link's or widget's `/A` entry.
impl FromPdfObject for AnnotationAction {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut action = context.dictionary()?;
        let action_type = action.required::<PdfName>(b"S")?;
        Ok(match action_type.as_ref() {
            b"GoTo" => Self::GoTo {
                destination: action.required(b"D")?,
            },
            b"GoToR" => Self::GoToRemote {
                file_specification: action.required(b"F")?,
                destination: action.optional(b"D")?,
                // Advisory viewer hint; like PDFium, a malformed value (e.g. the name
                // `/false`) is treated as absent rather than rejecting the annotation.
                new_window: action.lenient(b"NewWindow"),
            },
            b"URI" => Self::Uri {
                uri: action.required(b"URI")?,
                is_map: action.optional(b"IsMap")?,
            },
            b"Launch" => Self::Launch {
                file_specification: action.optional(b"F")?,
            },
            b"Named" => Self::Named {
                name: action.required(b"N")?,
            },
            b"SubmitForm" => Self::SubmitForm {
                file_specification: action.optional(b"F")?,
                fields: action.optional(b"Fields")?,
                flags: action.optional(b"Flags")?,
            },
            b"ResetForm" => Self::ResetForm {
                fields: action.optional(b"Fields")?,
                flags: action.optional(b"Flags")?,
            },
            b"ImportData" => Self::ImportData {
                file_specification: action.required(b"F")?,
            },
            b"JavaScript" => Self::JavaScript {
                script: action.required(b"JS")?,
            },
            b"SetOCGState" => Self::SetOCGState {
                state: ocg_state(ArrayContext::new(
                    &action.required::<PdfArray>(b"State")?,
                    action.access(),
                ))?,
                preserve_rb: action.optional(b"PreserveRB")?,
            },
            b"Rendition" => Self::Rendition {
                operation: action.optional(b"OP")?,
            },
            b"Trans" => Self::Trans {
                duration: action.optional(b"D")?,
            },
            b"GoTo3DView" => Self::GoTo3DView,
            other => Self::Unknown {
                action_type: Vec::from(other),
            },
        })
    }
}

/// Reads a named destination (a string or name) or an explicit destination array.
impl FromPdfObject for AnnotationDestination {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let object = context.object();
        match object {
            ObjectVariant::String(value) => Ok(Self::Named {
                name: value.as_bytes().to_vec(),
            }),
            ObjectVariant::Array(_) => Ok(Self::Explicit(Box::new(context.read(object)?))),
            other => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Array,
                actual: other.kind(),
            }),
        }
    }
}

/// Reads an explicit destination array `[page /Mode parameters...]`.
impl FromPdfObject for ExplicitDestination {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut items = context.array()?;
        let Some(page) = items.array().first() else {
            return Err(ObjectReadError::ArrayIndexOutOfBounds {
                index: 0,
                length: 0,
            });
        };
        let page = match items.is_absent(page)? {
            true => DestinationTarget::Null,
            false => DestinationTarget::try_from(page)?,
        };
        let mode = items.at::<PdfName>(1)?;
        Ok(match mode.as_ref() {
            b"XYZ" => Self::Xyz {
                page,
                left: items.optional_at(2)?,
                top: items.optional_at(3)?,
                zoom: items.optional_at(4)?,
            },
            b"Fit" => Self::Fit { page },
            b"FitH" => Self::FitH {
                page,
                top: items.optional_at(2)?,
            },
            b"FitV" => Self::FitV {
                page,
                left: items.optional_at(2)?,
            },
            b"FitR" => Self::FitR {
                page,
                left: items.at(2)?,
                bottom: items.at(3)?,
                right: items.at(4)?,
                top: items.at(5)?,
            },
            b"FitB" => Self::FitB { page },
            b"FitBH" => Self::FitBH {
                page,
                top: items.optional_at(2)?,
            },
            b"FitBV" => Self::FitBV {
                page,
                left: items.optional_at(2)?,
            },
            other => {
                return Err(SourceDecodeError::InvalidEntry {
                    entry: b"D",
                    reason: format!("unsupported destination type '{other:?}'"),
                }
                .into());
            }
        })
    }
}

/// Reads the page target of an explicit destination from its unresolved element.
///
/// A page reference keeps its identity, so the element is inspected as written rather
/// than decoded; a target that resolves to null is checked by the caller.
impl TryFrom<&ObjectVariant> for DestinationTarget {
    type Error = ObjectReadError;

    fn try_from(value: &ObjectVariant) -> ReadResult<Self> {
        Ok(match value {
            ObjectVariant::Reference(id) => Self::Reference {
                number: u64::try_from(id.number).map_err(|_| SourceDecodeError::ResourceLimit)?,
                generation: u64::try_from(id.generation)
                    .map_err(|_| SourceDecodeError::ResourceLimit)?,
            },
            ObjectVariant::Integer(index) => Self::PageIndex {
                index: u64::try_from(*index).map_err(|_| SourceDecodeError::ResourceLimit)?,
            },
            ObjectVariant::Dictionary(_) => Self::Dictionary,
            other => {
                return Err(ObjectReadError::TypeMismatch {
                    expected: ObjectKind::Dictionary,
                    actual: other.kind(),
                });
            }
        })
    }
}

/// Reads a file specification dictionary, or a string naming the file path.
impl FromPdfObject for FileSpecification {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        if !matches!(context.object(), ObjectVariant::Dictionary(_)) {
            return Ok(Self::Path(context.decode()?));
        }
        let mut dictionary = context.dictionary()?;
        Ok(Self::Dictionary(Box::new(FileSpecificationDictionary {
            file_system: dictionary.optional(b"FS")?,
            file_name: dictionary.optional(b"F")?,
            unicode_file_name: dictionary.optional(b"UF")?,
            mac_file_name: dictionary.optional(b"Mac")?,
            dos_file_name: dictionary.optional(b"DOS")?,
            unix_file_name: dictionary.optional(b"Unix")?,
            volatile: dictionary.optional(b"V")?,
        })))
    }
}

/// Decodes the `/State` array of a `SetOCGState` action.
///
/// Elements alternate between operation names and the group targets they govern,
/// so the sequence is retained in source order rather than collected as names.
/// Group references keep their unresolved identity, so elements are inspected raw.
fn ocg_state<A: ObjectAccess + ?Sized>(
    mut items: ArrayContext<'_, A>,
) -> ReadResult<Vec<OcgStateEntry>> {
    const ENTRY: &[u8] = b"State";

    let mut state = Vec::with_capacity(items.array().len());
    let mut seen_operation = false;
    let mut pending_operation = false;

    for item in items.array() {
        if item.is_name() {
            if pending_operation {
                return Err(SourceDecodeError::InvalidEntry {
                    entry: ENTRY,
                    reason: "optional content operation has no group target".to_owned(),
                }
                .into());
            }
            state.push(items.read::<OcgStateEntry>(item)?);
            seen_operation = true;
            pending_operation = true;
            continue;
        }

        if !seen_operation {
            return Err(SourceDecodeError::InvalidEntry {
                entry: ENTRY,
                reason: "group target precedes any optional content operation".to_owned(),
            }
            .into());
        }
        let target = match item {
            ObjectVariant::Reference(id) => OcgTarget::Reference {
                number: u64::try_from(id.number).map_err(|_| SourceDecodeError::ResourceLimit)?,
                generation: u64::try_from(id.generation)
                    .map_err(|_| SourceDecodeError::ResourceLimit)?,
            },
            _ => items.read(item)?,
        };
        state.push(OcgStateEntry::Group(target));
        pending_operation = false;
    }

    if pending_operation {
        return Err(SourceDecodeError::InvalidEntry {
            entry: ENTRY,
            reason: "optional content operation has no group target".to_owned(),
        }
        .into());
    }

    Ok(state)
}
