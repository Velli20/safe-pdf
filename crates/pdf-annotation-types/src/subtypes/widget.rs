use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{AnnotationAction, AnnotationError, AppearanceCharacteristics, BorderStyle, helpers};

/// A PDF widget field value from `/V` or `/DV`.
pub enum WidgetFieldValue {
    /// A string-like value represented as raw PDF bytes.
    Bytes(Vec<u8>),
    /// A dictionary value, such as a signature field value.
    Dictionary(Dictionary),
    /// An array of widget field values.
    Array(Vec<WidgetFieldValue>),
    /// An explicit PDF null value.
    Null,
}

/// Annotation-specific widget state.
pub struct WidgetAnnotation {
    /// The form field type.
    pub field_type: Option<Vec<u8>>,
    /// The field name.
    pub field_name: Option<Vec<u8>>,
    /// The alternate field name.
    pub alternate_name: Option<Vec<u8>>,
    /// The mapping name.
    pub mapping_name: Option<Vec<u8>>,
    /// The field flags.
    pub field_flags: Option<i32>,
    /// The value.
    pub value: Option<WidgetFieldValue>,
    /// The default value.
    pub default_value: Option<WidgetFieldValue>,
    /// The default appearance string.
    pub default_appearance: Option<Vec<u8>>,
    /// The quadding mode.
    pub quadding: Option<i32>,
    /// The appearance characteristics.
    pub appearance_characteristics: Option<AppearanceCharacteristics>,
    /// The border style.
    pub border_style: Option<BorderStyle>,
    /// The action dictionary.
    pub action: Option<AnnotationAction>,
    /// Additional actions.
    pub additional_actions: Option<Dictionary>,
}

impl WidgetAnnotation {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        let field_type = dictionary
            .get("FT")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let field_name = dictionary
            .get("T")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let alternate_name = dictionary
            .get("TU")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let mapping_name = dictionary
            .get("TM")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let field_flags = dictionary
            .get("Ff")
            .map(|value| value.try_number::<i32>(objects))
            .transpose()?;
        let value = dictionary
            .get("V")
            .map(|value| widget_field_value("V", value, objects))
            .transpose()?;
        let default_value = dictionary
            .get("DV")
            .map(|value| widget_field_value("DV", value, objects))
            .transpose()?;
        let default_appearance = dictionary
            .get("DA")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let quadding = dictionary
            .get("Q")
            .map(|value| value.try_number::<i32>(objects))
            .transpose()?;
        let additional_actions = dictionary
            .get("AA")
            .map(|value| helpers::dictionary(value, objects))
            .transpose()?;
        let appearance_characteristics =
            AppearanceCharacteristics::from_dictionary(dictionary, objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, "BS", objects)?;
        let action = AnnotationAction::from_dictionary(dictionary, "A", objects)?;

        Ok(Self {
            field_type,
            field_name,
            alternate_name,
            mapping_name,
            field_flags,
            value,
            default_value,
            default_appearance,
            quadding,
            appearance_characteristics,
            border_style,
            action,
            additional_actions,
        })
    }
}

fn widget_field_value(
    entry: &'static str,
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<WidgetFieldValue, AnnotationError> {
    let value = objects.resolve_object(value)?;
    match value {
        ObjectVariant::HexString(bytes)
        | ObjectVariant::LiteralString(bytes)
        | ObjectVariant::Name(bytes) => Ok(WidgetFieldValue::Bytes(bytes.clone())),
        ObjectVariant::Dictionary(dictionary) => {
            Ok(WidgetFieldValue::Dictionary(dictionary.as_ref().clone()))
        }
        ObjectVariant::Stream(stream) => Ok(WidgetFieldValue::Dictionary(
            stream.dictionary.as_ref().clone(),
        )),
        ObjectVariant::Array(values) => values
            .iter()
            .map(|value| widget_field_value(entry, value, objects))
            .collect::<Result<Vec<_>, _>>()
            .map(WidgetFieldValue::Array),
        ObjectVariant::Null => Ok(WidgetFieldValue::Null),
        other => Err(AnnotationError::InvalidEntry {
            entry,
            reason: format!(
                "expected string, name, dictionary, array, or null field value, found {}",
                other.name()
            ),
        }),
    }
}
