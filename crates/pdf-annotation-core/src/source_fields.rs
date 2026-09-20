//! Normalize one shared field from the widget records that display it.
//!
//! Every widget in a group must describe the same field; the first record supplies
//! the configuration and the others are checked against it. Buttons contribute one
//! option per distinct on-state, so each widget also learns which option it toggles.
use crate::{
    error::ValidationError,
    fields::{
        ButtonField, CheckboxGroupField, ChoiceOption, ComboBoxField, ComboValue, FieldId,
        FieldKind, ListBoxField, ListSelection, NewField, OptionId, RadioField, TextField,
    },
    metadata::{FieldMetadata, UnsupportedField},
    pdf_data::{
        NativeAnnotation, SourceAnnotation, WidgetAnnotation, WidgetFieldFlags, WidgetFieldValue,
    },
    source_properties::decode_text,
    widgets::WidgetBinding,
};

/// A field normalized from one source group, plus the option each widget toggles.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceFieldGroup {
    /// Shared field definition ready for `Operation::CreateField`.
    pub field: NewField,
    /// Per source widget, in input order: the toggle option it binds to, if any.
    pub options: Vec<Option<OptionId>>,
}

impl SourceFieldGroup {
    /// Binding for each source widget, in input order, once the field exists as `field`.
    ///
    /// Radio and checkbox groups bind every widget to its toggle option; other kinds
    /// bind the field itself. Does not panic.
    pub fn bindings(&self, field: FieldId) -> impl Iterator<Item = WidgetBinding> + '_ {
        self.options
            .iter()
            .map(move |option| match (&self.field.content, option) {
                (FieldKind::Radio(_), Some(option)) => WidgetBinding::RadioOption {
                    field,
                    option: *option,
                },
                (FieldKind::CheckboxGroup(_), Some(option)) => WidgetBinding::CheckboxOption {
                    field,
                    option: *option,
                },
                _ => WidgetBinding::Field { field },
            })
    }
}

/// Normalize the field shared by `sources`, which must all be widget records.
///
/// # Errors
/// Returns `InvalidSourceField` for an empty group, a non-widget member, widgets that
/// disagree on configuration or value, or button states that conflict; `InvalidTextString`
/// for undecodable names, captions, or labels; `InvalidMetadata` for index overflow.
pub fn resolve_source_field(
    sources: &[&SourceAnnotation],
) -> Result<SourceFieldGroup, ValidationError> {
    let widgets = widgets(sources)?;
    let first = *widgets.first().ok_or(ValidationError::InvalidSourceField {
        reason: "empty field group",
    })?;
    check_consistent(first, &widgets)?;
    let flags = first.field_flags.unwrap_or_default();
    let (content, options) = if first.is_push_button() {
        (push_button(first)?, vec![None; sources.len()])
    } else if first.is_button() {
        toggle_group(first, sources, &widgets)?
    } else {
        (field_kind(first, flags)?, vec![None; sources.len()])
    };
    Ok(SourceFieldGroup {
        field: NewField {
            metadata: FieldMetadata {
                source: Some(Box::new(first.clone())),
            },
            name: decode_text(first.field_name.as_deref())?,
            label: decode_text(
                first
                    .alternate_name
                    .as_deref()
                    .or(first.field_name.as_deref()),
            )?,
            read_only: first.is_read_only(),
            required: flags.contains(WidgetFieldFlags::REQUIRED),
            content,
        },
        options,
    })
}

/// Borrows every member's widget record, rejecting any other annotation kind.
fn widgets<'a>(
    sources: &[&'a SourceAnnotation],
) -> Result<Vec<&'a WidgetAnnotation>, ValidationError> {
    sources
        .iter()
        .map(|source| match &source.kind {
            NativeAnnotation::Widget(widget) => Ok(widget.as_ref()),
            _ => Err(ValidationError::InvalidSourceField {
                reason: "expected source widget",
            }),
        })
        .collect()
}

/// Every widget must repeat the first one's configuration and, unless a button, its value.
fn check_consistent(
    first: &WidgetAnnotation,
    widgets: &[&WidgetAnnotation],
) -> Result<(), ValidationError> {
    for other in widgets {
        if first.field_type != other.field_type
            || first.field_flags != other.field_flags
            || first.options != other.options
            || first.max_length != other.max_length
        {
            return Err(ValidationError::InvalidSourceField {
                reason: "inconsistent field configuration",
            });
        }
        if !first.is_button()
            && (first.value != other.value || first.selected_indices != other.selected_indices)
        {
            return Err(ValidationError::InvalidSourceField {
                reason: "inconsistent field values",
            });
        }
    }
    Ok(())
}

/// Live text value from `/V`; text and choice fields reject non-string values.
fn text_value(widget: &WidgetAnnotation) -> Result<String, ValidationError> {
    match &widget.value {
        None | Some(WidgetFieldValue::Null) => Ok(String::new()),
        Some(WidgetFieldValue::Bytes(bytes)) => decode_text(Some(bytes)),
        Some(WidgetFieldValue::Array(_)) if widget.is_multi_select() => Ok(String::new()),
        _ if matches!(widget.field_type.as_deref(), Some(b"Tx" | b"Ch")) => {
            Err(ValidationError::InvalidSourceField {
                reason: "non-text field value",
            })
        }
        _ => Ok(String::new()),
    }
}

/// Field content for the non-button field types named by `/FT`.
fn field_kind(
    widget: &WidgetAnnotation,
    flags: WidgetFieldFlags,
) -> Result<FieldKind, ValidationError> {
    match widget.field_type.as_deref() {
        Some(b"Tx") => text_field(widget, text_value(widget)?),
        Some(b"Ch") => choice_field(widget, flags, text_value(widget)?),
        _ => Ok(unsupported(widget)),
    }
}

/// Push button showing its normal caption from `/MK /CA`.
fn push_button(widget: &WidgetAnnotation) -> Result<FieldKind, ValidationError> {
    let caption = widget
        .appearance_characteristics
        .as_ref()
        .and_then(|mk| mk.normal_caption.as_deref());
    Ok(FieldKind::Button(ButtonField {
        caption: decode_text(caption)?,
    }))
}

/// Radio or checkbox group with one option per distinct on-state across the widgets.
fn toggle_group(
    first: &WidgetAnnotation,
    sources: &[&SourceAnnotation],
    widgets: &[&WidgetAnnotation],
) -> Result<(FieldKind, Vec<Option<OptionId>>), ValidationError> {
    // Checkboxes and radios-in-unison share an option between widgets with the same state.
    let share_states = !first.is_radio_button() || first.is_radios_in_unison();
    let mut toggles = ToggleOptions::default();
    let mut bindings = Vec::with_capacity(sources.len());
    for (source, widget) in sources.iter().zip(widgets) {
        let state = source.button_on_state()?;
        let option = toggles.option_for(state, share_states)?;
        if is_active(source, widget, state) {
            toggles.mark_active(option)?;
        }
        bindings.push(Some(option));
    }
    let content = if first.is_radio_button() {
        FieldKind::Radio(RadioField {
            options: toggles.options,
            selected: toggles.selected,
            allow_clear: !first.is_no_toggle_to_off(),
        })
    } else {
        FieldKind::CheckboxGroup(CheckboxGroupField {
            options: toggles.options,
            selected: toggles.selected,
        })
    };
    Ok((content, bindings))
}

/// Options discovered so far for a toggle group and which one is currently on.
#[derive(Default)]
struct ToggleOptions<'a> {
    states: Vec<&'a [u8]>,
    options: Vec<ChoiceOption>,
    selected: Option<OptionId>,
}

impl<'a> ToggleOptions<'a> {
    /// The option for `state`, reusing an earlier one only when `share_states` allows it.
    fn option_for(
        &mut self,
        state: &'a [u8],
        share_states: bool,
    ) -> Result<OptionId, ValidationError> {
        let repeated = share_states
            .then(|| self.states.iter().position(|known| *known == state))
            .flatten();
        if let Some(index) = repeated {
            return option_id(index);
        }
        let option = option_id(self.options.len())?;
        let label = decode_text(Some(state))?;
        self.states.push(state);
        self.options.push(ChoiceOption {
            id: option,
            export_value: label.clone(),
            label,
        });
        Ok(option)
    }

    /// Records `option` as the group's on-state; a second distinct on-state is a conflict.
    fn mark_active(&mut self, option: OptionId) -> Result<(), ValidationError> {
        if self.selected.is_some_and(|previous| previous != option) {
            return Err(ValidationError::InvalidSourceField {
                reason: "conflicting button appearance states",
            });
        }
        self.selected = Some(option);
        Ok(())
    }
}

/// Whether the widget currently shows `state`: by `/AS` when present, else by `/V`.
fn is_active(source: &SourceAnnotation, widget: &WidgetAnnotation, state: &[u8]) -> bool {
    match &source.appearance_state {
        Some(appearance) => appearance.as_slice() == state,
        None => matches!(&widget.value, Some(WidgetFieldValue::Bytes(v)) if v.as_slice() == state),
    }
}

/// Text field with its entry mode and optional length limit.
fn text_field(widget: &WidgetAnnotation, value: String) -> Result<FieldKind, ValidationError> {
    let max_length = widget
        .max_length
        .map(u32::try_from)
        .transpose()
        .map_err(|_| ValidationError::InvalidMetadata {
            field: "max length",
        })?;
    Ok(FieldKind::Text(TextField {
        mode: widget.text_mode(),
        max_length,
        value,
    }))
}

/// List box or combo box from `/Opt`, `/I` or `/V`, and the edit flag.
fn choice_field(
    widget: &WidgetAnnotation,
    flags: WidgetFieldFlags,
    value: String,
) -> Result<FieldKind, ValidationError> {
    let options = choice_options(widget)?;
    let selected = selected_options(widget, &options)?;
    if !widget.is_multi_select() && selected.len() > 1 {
        return Err(ValidationError::InvalidSourceField {
            reason: "multiple values in single choice",
        });
    }
    Ok(if widget.is_listbox() {
        list_box(widget, options, selected)
    } else {
        combo_box(flags, options, selected, value)
    })
}

/// Choice options in `/Opt` order, identified by their position.
fn choice_options(widget: &WidgetAnnotation) -> Result<Vec<ChoiceOption>, ValidationError> {
    widget
        .options
        .iter()
        .flatten()
        .enumerate()
        .map(|(index, option)| {
            Ok(ChoiceOption {
                id: option_id(index)?,
                label: decode_text(Some(&option.display_value))?,
                export_value: decode_text(Some(&option.export_value))?,
            })
        })
        .collect()
}

/// Identities of the source-selected options, which must all exist.
fn selected_options(
    widget: &WidgetAnnotation,
    options: &[ChoiceOption],
) -> Result<Vec<OptionId>, ValidationError> {
    widget
        .selected_option_indices()?
        .into_iter()
        .map(|index| {
            let index = usize::try_from(index).map_err(|_| ValidationError::InvalidMetadata {
                field: "option index",
            })?;
            options
                .get(index)
                .map(|option| option.id)
                .ok_or(ValidationError::InvalidSourceField {
                    reason: "choice option index",
                })
        })
        .collect()
}

fn list_box(
    widget: &WidgetAnnotation,
    options: Vec<ChoiceOption>,
    selected: Vec<OptionId>,
) -> FieldKind {
    let selection = if widget.is_multi_select() {
        ListSelection::Multiple(selected)
    } else {
        ListSelection::Single(selected.first().copied())
    };
    FieldKind::ListBox(ListBoxField { options, selection })
}

/// Combo whose value is a known option, custom text, or nothing.
fn combo_box(
    flags: WidgetFieldFlags,
    options: Vec<ChoiceOption>,
    selected: Vec<OptionId>,
    value: String,
) -> FieldKind {
    let value = match selected.first() {
        Some(id) => ComboValue::Option(*id),
        None if value.is_empty() => ComboValue::Empty,
        None => ComboValue::Text(value),
    };
    FieldKind::ComboBox(ComboBoxField {
        options,
        editable: flags.contains(WidgetFieldFlags::EDIT),
        value,
    })
}

/// Field types Core does not model keep their raw type and value for inspection.
fn unsupported(widget: &WidgetAnnotation) -> FieldKind {
    FieldKind::Unsupported(Box::new(UnsupportedField {
        field_type: widget.field_type.clone().unwrap_or_default(),
        value: widget.value.clone(),
    }))
}

/// Option identity for a position in the option list.
fn option_id(index: usize) -> Result<OptionId, ValidationError> {
    u32::try_from(index)
        .map(OptionId)
        .map_err(|_| ValidationError::InvalidMetadata {
            field: "option index",
        })
}
