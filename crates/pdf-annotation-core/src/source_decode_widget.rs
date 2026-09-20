//! Widget decoding, including field properties inherited through `/Parent`.
use crate::error::SourceDecodeError;
use crate::pdf_data::{
    AnnotationAction, AppearanceCharacteristics, BorderStyle, WidgetAnnotation, WidgetChoiceOption,
    WidgetFieldFlags, WidgetFieldValue,
};
use crate::source_decode::DecodeResult;
use pdf_object_reader::{
    dictionary::Dictionary,
    object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
    parent_chain::{ParentChain, parent_reference},
};

impl WidgetAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let field_id = parent_reference(dictionary)
            .map(|(_, parent_id)| {
                u64::try_from(parent_id.number).map_err(|_| SourceDecodeError::ResourceLimit)
            })
            .transpose()?;
        let fields = FieldProperties::inherit(dictionary, objects)?;

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
            appearance_characteristics: AppearanceCharacteristics::from_dictionary(
                dictionary, objects,
            )?,
            border_style: BorderStyle::from_dictionary(dictionary, b"BS", objects)?,
            action: AnnotationAction::from_dictionary(dictionary, b"A", objects)?,
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

impl FieldProperties {
    /// Collects field entries from the widget and its `/Parent` chain; nearer nodes win.
    fn inherit(dictionary: &Dictionary, objects: &dyn ObjectResolver) -> DecodeResult<Self> {
        let mut properties = Self::default();
        for node in ParentChain::new(dictionary, objects) {
            properties.absorb(node?, objects)?;
        }
        Ok(properties)
    }

    /// Records every field entry of `dictionary` that a nearer node has not already defined.
    fn absorb(
        &mut self,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<()> {
        for (key, value) in dictionary.iter() {
            let value = objects.resolve_object(value)?;
            if value == &ObjectVariant::Null {
                continue;
            }

            match key {
                b"FT" if self.field_type.is_none() => {
                    self.field_type = Some(value.try_bytes_vec(objects)?);
                }
                b"T" if self.field_name.is_none() => {
                    self.field_name = Some(value.try_bytes_vec(objects)?);
                }
                b"TU" if self.alternate_name.is_none() => {
                    self.alternate_name = Some(value.try_bytes_vec(objects)?);
                }
                b"TM" if self.mapping_name.is_none() => {
                    self.mapping_name = Some(value.try_bytes_vec(objects)?);
                }
                b"Ff" if self.field_flags.is_none() => {
                    self.field_flags = Some(
                        value
                            .try_number(objects)
                            .map(WidgetFieldFlags::from_bits_truncate)?,
                    );
                }
                b"V" if self.value.is_none() => {
                    self.value = Some(widget_field_value(b"V", value, objects)?);
                }
                b"DV" if self.default_value.is_none() => {
                    self.default_value = Some(widget_field_value(b"DV", value, objects)?);
                }
                b"DA" if self.default_appearance.is_none() => {
                    self.default_appearance = Some(value.try_bytes_vec(objects)?);
                }
                b"Q" if self.quadding.is_none() => {
                    self.quadding = Some(value.try_number(objects)?);
                }
                b"MaxLen" if self.max_length.is_none() => {
                    self.max_length = Some(value.try_number(objects)?);
                }
                b"Opt" if self.options.is_none() => {
                    self.options = Some(choice_options(value, objects)?);
                }
                b"I" if self.selected_indices.is_none() => {
                    self.selected_indices = Some(choice_indices(value, objects)?);
                }
                b"TI" if self.top_index.is_none() => {
                    self.top_index = Some(value.try_number::<u64>(objects)?);
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn choice_options(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> DecodeResult<Vec<WidgetChoiceOption>> {
    let values = value.try_array(objects)?;
    values
        .iter()
        .map(|value| {
            let resolved = objects.resolve_object(value)?;
            if let Ok(bytes) = resolved.try_bytes_vec(objects) {
                return Ok(WidgetChoiceOption {
                    export_value: bytes.clone(),
                    display_value: bytes,
                });
            }

            let pair = resolved.try_array(objects)?;
            if pair.len() != 2 {
                return Err(SourceDecodeError::InvalidEntry {
                    entry: b"Opt",
                    reason: "expected each option pair to contain an export and display value"
                        .to_owned(),
                });
            }
            let export_value = pair
                .first()
                .ok_or_else(|| SourceDecodeError::InvalidEntry {
                    entry: b"Opt",
                    reason: "missing option export value".to_owned(),
                })?
                .try_bytes_vec(objects)?;
            let display_value = pair
                .get(1)
                .ok_or_else(|| SourceDecodeError::InvalidEntry {
                    entry: b"Opt",
                    reason: "missing option display value".to_owned(),
                })?
                .try_bytes_vec(objects)?;
            Ok(WidgetChoiceOption {
                export_value,
                display_value,
            })
        })
        .collect()
}

fn choice_indices(value: &ObjectVariant, objects: &dyn ObjectResolver) -> DecodeResult<Vec<u64>> {
    value
        .try_array(objects)?
        .iter()
        .map(|value| choice_index(value, objects))
        .collect()
}

fn choice_index(value: &ObjectVariant, objects: &dyn ObjectResolver) -> DecodeResult<u64> {
    let value = value.try_number::<u64>(objects)?;
    Ok(value)
}

fn widget_field_value(
    entry: &'static [u8],
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> DecodeResult<WidgetFieldValue> {
    let value = objects.resolve_object(value)?;
    match value {
        ObjectVariant::String(bytes) => Ok(WidgetFieldValue::Bytes(bytes.as_bytes().to_vec())),
        ObjectVariant::Dictionary(_) | ObjectVariant::Stream(_) => Ok(WidgetFieldValue::Dictionary),
        ObjectVariant::Array(values) => values
            .iter()
            .map(|value| widget_field_value(entry, value, objects))
            .collect::<Result<Vec<_>, _>>()
            .map(WidgetFieldValue::Array),
        ObjectVariant::Null => Ok(WidgetFieldValue::Null),
        other => Err(SourceDecodeError::InvalidEntry {
            entry,
            reason: format!(
                "expected string, name, dictionary, array, or null field value, found {}",
                other.name()
            ),
        }),
    }
}
