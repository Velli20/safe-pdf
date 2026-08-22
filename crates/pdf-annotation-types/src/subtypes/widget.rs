use std::collections::BTreeSet;

use bitflags::bitflags;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

use crate::{
    Annotation, AnnotationAction, AnnotationError, AppearanceCharacteristics, BorderStyle, helpers,
};

bitflags! {
    /// Widget form field flags parsed from the inherited `/Ff` entry.
    ///
    /// Unknown bits are preserved with `from_bits_retain` so future PDF flags
    /// round-trip without being dropped.
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct WidgetFieldFlags: i32 {
        /// The user may not change the value of the field.
        const READ_ONLY = 1 << 0;
        /// A selected radio button may not be toggled off by user interaction.
        const NO_TOGGLE_TO_OFF = 1 << 14;
        /// The widget belongs to a radio button field.
        const RADIO_BUTTON = 1 << 15;
        /// The widget is a push button.
        const PUSH_BUTTON = 1 << 16;
        /// The choice field is a combo box rather than a listbox.
        const COMBO_BOX = 1 << 17;
        /// The choice field permits more than one selected option.
        const MULTI_SELECT = 1 << 21;
        /// Radio buttons with the same on-state value change in unison.
        const RADIOS_IN_UNISON = 1 << 25;
    }
}

/// One option in a PDF choice field's `/Opt` array.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WidgetChoiceOption {
    /// Value written to the field when this option is selected.
    pub export_value: Vec<u8>,
    /// Value presented to the user for this option.
    pub display_value: Vec<u8>,
}

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
    /// Indirect object number of the terminal field that owns this widget.
    ///
    /// Widgets belonging to the same logical field share this identifier even
    /// when other field properties are inherited from higher ancestors.
    pub field_id: Option<usize>,
    /// The form field type.
    pub field_type: Option<Vec<u8>>,
    /// The field name.
    pub field_name: Option<Vec<u8>>,
    /// The alternate field name.
    pub alternate_name: Option<Vec<u8>>,
    /// The mapping name.
    pub mapping_name: Option<Vec<u8>>,
    /// The field flags.
    pub field_flags: Option<WidgetFieldFlags>,
    /// The value.
    pub value: Option<WidgetFieldValue>,
    /// The default value.
    pub default_value: Option<WidgetFieldValue>,
    /// The default appearance string.
    pub default_appearance: Option<Vec<u8>>,
    /// The quadding mode.
    pub quadding: Option<i32>,
    /// Options available to a choice field.
    pub options: Option<Vec<WidgetChoiceOption>>,
    /// Explicitly selected choice option indices from `/I`.
    pub selected_indices: Option<Vec<usize>>,
    /// Index of the first visible listbox option from `/TI`.
    pub top_index: Option<usize>,
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
        let field_id = match dictionary.get(b"Parent") {
            Some(ObjectVariant::Reference(parent_id)) => Some(*parent_id),
            _ => None,
        };
        let mut field_type = None;
        let mut field_name = None;
        let mut alternate_name = None;
        let mut mapping_name = None;
        let mut field_flags = None;
        let mut value = None;
        let mut default_value = None;
        let mut default_appearance = None;
        let mut quadding = None;
        let mut options = None;
        let mut selected_indices = None;
        let mut top_index = None;
        let mut current = dictionary;
        let mut visited_parents = BTreeSet::new();

        loop {
            if field_type.is_none() {
                field_type = current.optional_bytes(b"FT", objects)?.map(Vec::from);
            }
            if field_name.is_none() {
                field_name = current.optional_bytes_vec(b"T", objects)?;
            }
            if alternate_name.is_none() {
                alternate_name = current.optional_bytes_vec(b"TU", objects)?;
            }
            if mapping_name.is_none() {
                mapping_name = current.optional_bytes_vec(b"TM", objects)?;
            }
            if field_flags.is_none() {
                field_flags = current
                    .optional_number::<i32>(b"Ff", objects)?
                    .map(WidgetFieldFlags::from_bits_retain);
            }
            if value.is_none() {
                value = current
                    .get(b"V")
                    .map(|value| widget_field_value(b"V", value, objects))
                    .transpose()?;
            }
            if default_value.is_none() {
                default_value = current
                    .get(b"DV")
                    .map(|value| widget_field_value(b"DV", value, objects))
                    .transpose()?;
            }
            if default_appearance.is_none() {
                default_appearance = current.optional_bytes_vec(b"DA", objects)?;
            }
            if quadding.is_none() {
                quadding = current.optional_number::<i32>(b"Q", objects)?;
            }
            if options.is_none()
                && let Some(value) = current.get(b"Opt")
            {
                options = Some(choice_options(value, objects)?);
            }
            if selected_indices.is_none()
                && let Some(value) = current.get(b"I")
            {
                selected_indices = Some(choice_indices(b"I", value, objects)?);
            }
            if top_index.is_none()
                && let Some(value) = current.get(b"TI")
            {
                top_index = Some(choice_index(b"TI", value, objects)?);
            }

            let Some(parent @ ObjectVariant::Reference(parent_id)) = current.get(b"Parent") else {
                break;
            };
            if !visited_parents.insert(*parent_id) {
                break;
            }
            current = parent.try_dictionary(objects)?;
        }

        let additional_actions = dictionary
            .get(b"AA")
            .map(|value| helpers::dictionary(value, objects))
            .transpose()?;

        let appearance_characteristics =
            AppearanceCharacteristics::from_dictionary(dictionary, objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let action = AnnotationAction::from_dictionary(dictionary, b"A", objects)?;

        Ok(Self {
            field_id,
            field_type,
            field_name,
            alternate_name,
            mapping_name,
            field_flags,
            value,
            default_value,
            default_appearance,
            quadding,
            options,
            selected_indices,
            top_index,
            appearance_characteristics,
            border_style,
            action,
            additional_actions,
        })
    }

    /// Returns whether this widget belongs to a button field.
    #[must_use]
    pub fn is_button(&self) -> bool {
        self.field_type.as_deref() == Some(b"Btn")
    }

    /// Returns this button widget's active non-`/Off` appearance state.
    #[must_use]
    pub fn active_button_state<'a>(&self, annotation: &'a Annotation) -> Option<&'a [u8]> {
        annotation
            .appearance_state
            .as_deref()
            .filter(|state| *state != b"Off")
    }

    /// Returns whether this widget is a checkbox button.
    #[must_use]
    pub fn is_checkbox(&self) -> bool {
        self.is_button() && !self.is_push_button() && !self.is_radio_button()
    }

    /// Returns whether the widget is marked as a radio button in `/Ff`.
    ///
    /// This check only inspects the widget flags. Callers should still verify
    /// the field type when they need a complete `/Btn` classification.
    #[must_use]
    pub fn is_radio_button(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::RADIO_BUTTON))
    }

    /// Returns whether the widget is marked as a push button in `/Ff`.
    ///
    /// This check only inspects the widget flags. Callers should still verify
    /// the field type when they need a complete `/Btn` classification.
    #[must_use]
    pub fn is_push_button(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::PUSH_BUTTON))
    }

    /// Returns whether this choice field is a combo box rather than a listbox.
    #[must_use]
    pub fn is_combo_box(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::COMBO_BOX))
    }

    /// Returns whether this widget belongs to a listbox choice field.
    #[must_use]
    pub fn is_listbox(&self) -> bool {
        self.field_type.as_deref() == Some(b"Ch") && !self.is_combo_box()
    }

    /// Returns whether this choice field permits multiple selected options.
    #[must_use]
    pub fn is_multi_select(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::MULTI_SELECT))
    }

    /// Returns selected option indices, preferring `/I` over values inferred from `/V`.
    #[must_use]
    pub fn selected_option_indices(&self) -> Vec<usize> {
        if let Some(indices) = &self.selected_indices {
            return indices.clone();
        }
        let Some(options) = &self.options else {
            return Vec::new();
        };

        let mut indices = match &self.value {
            Some(WidgetFieldValue::Bytes(value)) => {
                option_index(options, value).into_iter().collect()
            }
            Some(WidgetFieldValue::Array(values)) => values
                .iter()
                .filter_map(|value| {
                    let WidgetFieldValue::Bytes(value) = value else {
                        return None;
                    };
                    option_index(options, value)
                })
                .collect(),
            _ => Vec::new(),
        };
        indices.sort_unstable();
        indices.dedup();
        indices
    }

    /// Returns whether user interaction may change this field's value.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::READ_ONLY))
    }

    /// Returns whether a selected radio button must remain selected.
    #[must_use]
    pub fn is_no_toggle_to_off(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::NO_TOGGLE_TO_OFF))
    }

    /// Returns whether radio buttons sharing an on-state change in unison.
    #[must_use]
    pub fn is_radios_in_unison(&self) -> bool {
        self.field_flags
            .is_some_and(|flags| flags.contains(WidgetFieldFlags::RADIOS_IN_UNISON))
    }
}

fn choice_options(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Vec<WidgetChoiceOption>, AnnotationError> {
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
                return Err(AnnotationError::InvalidEntry {
                    entry: b"Opt",
                    reason: "expected each option pair to contain an export and display value"
                        .to_owned(),
                });
            }
            let export_value = pair
                .first()
                .ok_or_else(|| AnnotationError::InvalidEntry {
                    entry: b"Opt",
                    reason: "missing option export value".to_owned(),
                })?
                .try_bytes_vec(objects)?;
            let display_value = pair
                .get(1)
                .ok_or_else(|| AnnotationError::InvalidEntry {
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

fn choice_indices(
    entry: &'static [u8],
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Vec<usize>, AnnotationError> {
    value
        .try_array(objects)?
        .iter()
        .map(|value| choice_index(entry, value, objects))
        .collect()
}

fn choice_index(
    entry: &'static [u8],
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<usize, AnnotationError> {
    let value = value.try_number::<i64>(objects)?;
    usize::try_from(value).map_err(|_| AnnotationError::InvalidEntry {
        entry,
        reason: "expected a non-negative option index".to_owned(),
    })
}

fn option_index(options: &[WidgetChoiceOption], value: &[u8]) -> Option<usize> {
    options
        .iter()
        .position(|option| option.export_value == value)
}

fn widget_field_value(
    entry: &'static [u8],
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<WidgetFieldValue, AnnotationError> {
    let value = objects.resolve_object(value)?;
    match value {
        ObjectVariant::HexString(bytes)
        | ObjectVariant::LiteralString(bytes)
        | ObjectVariant::Name(bytes) => Ok(WidgetFieldValue::Bytes(bytes.clone())),
        ObjectVariant::Dictionary(dictionary) => {
            Ok(WidgetFieldValue::Dictionary(dictionary.clone()))
        }
        ObjectVariant::Stream(stream) => {
            Ok(WidgetFieldValue::Dictionary(stream.dictionary.clone()))
        }
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

#[cfg(test)]
mod tests {
    use super::{WidgetAnnotation, WidgetFieldFlags};

    fn widget(field_flags: Option<WidgetFieldFlags>) -> WidgetAnnotation {
        WidgetAnnotation {
            field_id: None,
            field_type: None,
            field_name: None,
            alternate_name: None,
            mapping_name: None,
            field_flags,
            value: None,
            default_value: None,
            default_appearance: None,
            quadding: None,
            options: None,
            selected_indices: None,
            top_index: None,
            appearance_characteristics: None,
            border_style: None,
            action: None,
            additional_actions: None,
        }
    }

    #[test]
    fn recognizes_radio_button_flag() {
        assert!(widget(Some(WidgetFieldFlags::RADIO_BUTTON)).is_radio_button());
        assert!(!widget(Some(WidgetFieldFlags::RADIO_BUTTON)).is_push_button());
    }

    #[test]
    fn recognizes_push_button_flag() {
        assert!(widget(Some(WidgetFieldFlags::PUSH_BUTTON)).is_push_button());
        assert!(!widget(Some(WidgetFieldFlags::PUSH_BUTTON)).is_radio_button());
    }

    #[test]
    fn classifies_button_widgets() {
        let mut checkbox = widget(None);
        checkbox.field_type = Some(b"Btn".to_vec());
        assert!(checkbox.is_button());
        assert!(checkbox.is_checkbox());

        let mut radio = widget(Some(WidgetFieldFlags::RADIO_BUTTON));
        radio.field_type = Some(b"Btn".to_vec());
        assert!(radio.is_button());
        assert!(!radio.is_checkbox());

        let mut push = widget(Some(WidgetFieldFlags::PUSH_BUTTON));
        push.field_type = Some(b"Btn".to_vec());
        assert!(push.is_button());
        assert!(!push.is_checkbox());

        assert!(!widget(None).is_button());
        assert!(!widget(None).is_checkbox());
    }

    #[test]
    fn classifies_listbox_widgets() {
        let mut listbox = widget(None);
        listbox.field_type = Some(b"Ch".to_vec());
        assert!(listbox.is_listbox());

        let mut combo_box = widget(Some(WidgetFieldFlags::COMBO_BOX));
        combo_box.field_type = Some(b"Ch".to_vec());
        assert!(!combo_box.is_listbox());

        assert!(!widget(None).is_listbox());
    }

    #[test]
    fn recognizes_interaction_flags() {
        let widget = widget(Some(
            WidgetFieldFlags::READ_ONLY
                | WidgetFieldFlags::NO_TOGGLE_TO_OFF
                | WidgetFieldFlags::RADIOS_IN_UNISON,
        ));

        assert!(widget.is_read_only());
        assert!(widget.is_no_toggle_to_off());
        assert!(widget.is_radios_in_unison());
    }

    #[test]
    fn preserves_unknown_flags() {
        let flags = WidgetFieldFlags::from_bits_retain((1 << 0) | (1 << 15));
        assert!(flags.contains(WidgetFieldFlags::RADIO_BUTTON));
        assert_eq!(flags.bits(), (1 << 0) | (1 << 15));
    }
}
