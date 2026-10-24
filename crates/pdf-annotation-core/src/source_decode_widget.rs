//! Widget decoding, including field properties inherited through `/Parent`.
use std::sync::Arc;

use crate::error::SourceDecodeError;
use crate::pdf_data::{WidgetAnnotation, WidgetChoiceOption, WidgetFieldFlags, WidgetFieldValue};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};

impl FromPdfObject for WidgetAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut dictionary = context.dictionary()?;
        let field_id = dictionary
            .reference(b"Parent")
            .map(|parent_id| {
                u64::try_from(parent_id.number).map_err(|_| SourceDecodeError::ResourceLimit)
            })
            .transpose()?;
        let fields: FieldProperties = dictionary.read(object)?;

        Ok(Self {
            field_id,
            field_type: fields.field_type,
            field_name: fields.field_name,
            alternate_name: fields.alternate_name,
            mapping_name: fields.mapping_name,
            field_flags: fields.field_flags,
            value: fields.value,
            default_value: fields.default_value,
            default_appearance: fields.default_appearance,
            quadding: fields.quadding,
            max_length: fields.max_length,
            options: fields.options,
            selected_indices: fields.selected_indices,
            top_index: fields.top_index,
            appearance_characteristics: dictionary.optional(b"MK")?,
            border_style: dictionary.optional(b"BS")?,
            action: dictionary.optional(b"A")?,
        })
    }
}

/// Field entries a widget may inherit from its `/Parent` chain.
#[derive(Default)]
struct FieldProperties {
    field_type: Option<Vec<u8>>,
    field_name: Option<Vec<u8>>,
    alternate_name: Option<Vec<u8>>,
    mapping_name: Option<Vec<u8>>,
    field_flags: Option<WidgetFieldFlags>,
    value: Option<WidgetFieldValue>,
    default_value: Option<WidgetFieldValue>,
    default_appearance: Option<Vec<u8>>,
    quadding: Option<i32>,
    max_length: Option<u64>,
    options: Option<Vec<WidgetChoiceOption>>,
    selected_indices: Option<Vec<u64>>,
    top_index: Option<u64>,
}

/// Reads the field entries of a widget or field node, inheriting each absent entry from
/// its `/Parent` chain; nearer nodes win. A cyclic `/Parent` chain ends at the repeated node.
impl FromPdfObject for FieldProperties {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        let parent = match dictionary.optional::<Self>(b"Parent") {
            Err(error) if error.is_cyclic_reference() => None,
            result => result?,
        }
        .unwrap_or_default();

        Ok(Self {
            field_type: dictionary
                .optional::<Arc<[u8]>>(b"FT")?
                .map(|bytes| bytes.to_vec())
                .or(parent.field_type),
            field_name: dictionary
                .optional::<Arc<[u8]>>(b"T")?
                .map(|bytes| bytes.to_vec())
                .or(parent.field_name),
            alternate_name: dictionary
                .optional::<Arc<[u8]>>(b"TU")?
                .map(|bytes| bytes.to_vec())
                .or(parent.alternate_name),
            mapping_name: dictionary
                .optional::<Arc<[u8]>>(b"TM")?
                .map(|bytes| bytes.to_vec())
                .or(parent.mapping_name),
            field_flags: dictionary
                .optional::<i32>(b"Ff")?
                .map(WidgetFieldFlags::from_bits_truncate)
                .or(parent.field_flags),
            value: dictionary.optional(b"V")?.or(parent.value),
            default_value: dictionary.optional(b"DV")?.or(parent.default_value),
            default_appearance: dictionary
                .optional::<Arc<[u8]>>(b"DA")?
                .map(|bytes| bytes.to_vec())
                .or(parent.default_appearance),
            quadding: dictionary.optional(b"Q")?.or(parent.quadding),
            max_length: dictionary.optional(b"MaxLen")?.or(parent.max_length),
            options: dictionary.optional(b"Opt")?.or(parent.options),
            selected_indices: dictionary.optional(b"I")?.or(parent.selected_indices),
            top_index: dictionary.optional(b"TI")?.or(parent.top_index),
        })
    }
}

/// Reads one `/Opt` entry: a single string, or an `[export display]` pair.
impl FromPdfObject for WidgetChoiceOption {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        if !matches!(context.object(), ObjectVariant::Array(_)) {
            let bytes = context.decode::<Arc<[u8]>>()?.to_vec();
            return Ok(Self {
                export_value: bytes.clone(),
                display_value: bytes,
            });
        }
        let mut pair = context.array()?;
        if pair.array().len() != 2 {
            return Err(SourceDecodeError::InvalidEntry {
                entry: b"Opt",
                reason: "expected each option pair to contain an export and display value"
                    .to_owned(),
            }
            .into());
        }
        Ok(Self {
            export_value: pair.at::<Arc<[u8]>>(0)?.to_vec(),
            display_value: pair.at::<Arc<[u8]>>(1)?.to_vec(),
        })
    }
}

/// Reads a `/V` or `/DV` field value; dictionary values are recorded without their contents.
impl FromPdfObject for WidgetFieldValue {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::String(bytes) => Ok(Self::Bytes(bytes.as_bytes().to_vec())),
            ObjectVariant::Dictionary(_) | ObjectVariant::Stream(_) => Ok(Self::Dictionary),
            ObjectVariant::Array(_) => context.array()?.read_all().map(Self::Array),
            ObjectVariant::Null => Ok(Self::Null),
            other => Err(SourceDecodeError::InvalidEntry {
                entry: b"V",
                reason: format!(
                    "expected string, name, dictionary, array, or null field value, found {}",
                    other.name()
                ),
            }
            .into()),
        }
    }
}
