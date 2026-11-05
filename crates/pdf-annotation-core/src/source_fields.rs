//! Normalize one shared field from the widget records that display it.
//!
//! Every widget in a group must describe the same field; the first record supplies
//! the configuration and the others are checked against it. Buttons contribute one
//! option per distinct on-state, so each widget also learns which option it toggles.
use crate::{
    error::SourceAdoptionError,
    fields::{
        ButtonField, CheckboxGroupField, ChoiceOption, ComboBoxField, ComboValue, FieldId,
        FieldKind, ListBoxField, ListSelection, NewField, OptionId, RadioField, TextField,
    },
    metadata::{FieldMetadata, UnsupportedField},
    pdf_data::{SourceAnnotation, WidgetAnnotation, WidgetFieldValue},
    source_properties::decode_text,
    widgets::WidgetBinding,
};
use std::collections::HashMap;

/// A field normalized from one source group, plus how each widget displays it.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceFieldGroup {
    /// Shared field definition ready for `Operation::CreateField`.
    pub field: NewField,
    /// How the source widgets bind to the field once it exists.
    pub bindings: GroupBindings,
}

/// How a group's widgets display its field.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupBindings {
    /// Every widget binds the field itself.
    Field,
    /// Per source widget, in input order: the radio option it toggles.
    Radio(Vec<OptionId>),
    /// Per source widget, in input order: the checkbox option it toggles.
    Checkbox(Vec<OptionId>),
}

impl GroupBindings {
    /// The binding of the source widget at `index` once the field exists as `field`,
    /// or `None` past the end of a toggle group.
    pub fn binding(&self, field: FieldId, index: usize) -> Option<WidgetBinding> {
        Some(match self {
            Self::Field => WidgetBinding::Field { field },
            Self::Radio(options) => WidgetBinding::RadioOption {
                field,
                option: *options.get(index)?,
            },
            Self::Checkbox(options) => WidgetBinding::CheckboxOption {
                field,
                option: *options.get(index)?,
            },
        })
    }
}

/// Normalizes the field shared by the sources, which must all be widget records.
///
/// Returns `InvalidSourceField` for an empty group, a non-widget member, widgets that
/// disagree on configuration or value, or button states that conflict; `InvalidTextString`
/// for undecodable names, captions, or labels; `IndexOverflow` for index overflow.
impl TryFrom<&[&SourceAnnotation]> for SourceFieldGroup {
    type Error = SourceAdoptionError;

    fn try_from(sources: &[&SourceAnnotation]) -> Result<Self, Self::Error> {
        let members = sources
            .iter()
            .map(|source| FieldMember::try_from(*source))
            .collect::<Result<Vec<_>, _>>()?;
        let [FieldMember { widget: first, .. }, rest @ ..] = members.as_slice() else {
            return Err(SourceAdoptionError::InvalidSourceField {
                reason: "empty field group",
            });
        };
        first.check_consistent(rest)?;
        let (content, bindings) = if first.is_toggle_button() {
            first.toggle_group(&members)?
        } else {
            (first.field_kind()?, GroupBindings::Field)
        };
        let name = decode_text(first.field_name.as_deref())?;
        let label = match first.alternate_name.as_deref() {
            Some(alternate_name) => decode_text(Some(alternate_name))?,
            None => name.clone(),
        };
        Ok(Self {
            field: NewField {
                metadata: FieldMetadata {
                    source: Some(Box::new((*first).clone())),
                },
                name,
                label,
                read_only: first.is_read_only(),
                required: first.is_required(),
                content,
            },
            bindings,
        })
    }
}

/// One group member with its widget record.
struct FieldMember<'a> {
    source: &'a SourceAnnotation,
    widget: &'a WidgetAnnotation,
}

/// Rejects any annotation kind other than a widget.
impl<'a> TryFrom<&'a SourceAnnotation> for FieldMember<'a> {
    type Error = SourceAdoptionError;

    fn try_from(source: &'a SourceAnnotation) -> Result<Self, Self::Error> {
        let widget = source
            .widget()
            .ok_or(SourceAdoptionError::InvalidSourceField {
                reason: "expected source widget",
            })?;
        Ok(Self { source, widget })
    }
}

impl FieldMember<'_> {
    /// Whether the widget currently shows `state`: by `/AS` when present, else by `/V`.
    fn is_active(&self, state: &[u8]) -> bool {
        match &self.source.appearance_state {
            Some(appearance) => **appearance == *state,
            None => matches!(&self.widget.value, Some(WidgetFieldValue::Bytes(v)) if **v == *state),
        }
    }
}

impl WidgetAnnotation {
    /// `others` must repeat this widget's configuration and, unless a button, its value.
    fn check_consistent(&self, others: &[FieldMember<'_>]) -> Result<(), SourceAdoptionError> {
        let is_button = self.is_button();
        for FieldMember { widget: other, .. } in others {
            if self.field_type != other.field_type
                || self.field_flags != other.field_flags
                || self.options != other.options
                || self.max_length != other.max_length
            {
                return Err(SourceAdoptionError::InvalidSourceField {
                    reason: "inconsistent field configuration",
                });
            }
            if !is_button
                && (self.value != other.value || self.selected_indices != other.selected_indices)
            {
                return Err(SourceAdoptionError::InvalidSourceField {
                    reason: "inconsistent field values",
                });
            }
        }
        Ok(())
    }

    /// Field content for every `/FT` but toggle buttons, which span the group's widgets.
    fn field_kind(&self) -> Result<FieldKind, SourceAdoptionError> {
        match self.field_type.as_deref() {
            Some(b"Btn") => {
                let caption = self
                    .appearance_characteristics
                    .as_ref()
                    .and_then(|mk| mk.normal_caption.as_deref());
                Ok(FieldKind::Button(ButtonField {
                    caption: decode_text(caption)?,
                }))
            }
            Some(b"Tx") => Ok(FieldKind::Text(TextField {
                mode: self.text_mode(),
                max_length: self.max_length,
                value: self.text_value()?,
            })),
            Some(b"Ch") => self.choice_field(),
            // Field types Core does not model keep their raw type and value for inspection.
            _ => Ok(FieldKind::Unsupported(Box::new(UnsupportedField {
                field_type: self.field_type.clone().unwrap_or_default(),
                value: self.value.clone(),
            }))),
        }
    }

    /// Live text value from `/V` of a text or choice field.
    fn text_value(&self) -> Result<String, SourceAdoptionError> {
        match &self.value {
            None | Some(WidgetFieldValue::Null) => Ok(String::new()),
            Some(WidgetFieldValue::Bytes(bytes)) => decode_text(Some(bytes)),
            Some(WidgetFieldValue::Array(_)) if self.is_multi_select() => Ok(String::new()),
            _ => Err(SourceAdoptionError::InvalidSourceField {
                reason: "non-text field value",
            }),
        }
    }

    /// Radio or checkbox group with one option per distinct on-state across `members`.
    fn toggle_group(
        &self,
        members: &[FieldMember<'_>],
    ) -> Result<(FieldKind, GroupBindings), SourceAdoptionError> {
        // Checkboxes and radios-in-unison share an option between widgets with the same state.
        let share_states = !self.is_radio_button() || self.is_radios_in_unison();
        let mut toggles = ToggleOptions::default();
        let mut bindings = Vec::with_capacity(members.len());
        for member in members {
            let state = member.source.button_on_state()?;
            let option = toggles.option_for(state, share_states)?;
            if member.is_active(state) {
                toggles.mark_active(option)?;
            }
            bindings.push(option);
        }
        let ToggleOptions {
            options, selected, ..
        } = toggles;
        Ok(if self.is_radio_button() {
            let radio = RadioField {
                options,
                selected,
                allow_clear: !self.is_no_toggle_to_off(),
            };
            (FieldKind::Radio(radio), GroupBindings::Radio(bindings))
        } else {
            let group = CheckboxGroupField { options, selected };
            (
                FieldKind::CheckboxGroup(group),
                GroupBindings::Checkbox(bindings),
            )
        })
    }

    /// List box or combo box from `/Opt`, `/I` or `/V`, and the edit flag; only combos
    /// read `/V` text.
    fn choice_field(&self) -> Result<FieldKind, SourceAdoptionError> {
        let options = self.choice_options()?;
        let selected = self.selected_options(&options)?;
        if !self.is_multi_select() && selected.len() > 1 {
            return Err(SourceAdoptionError::InvalidSourceField {
                reason: "multiple values in single choice",
            });
        }
        if self.is_listbox() {
            Ok(self.list_box(options, selected))
        } else {
            Ok(self.combo_box(options, selected, self.text_value()?))
        }
    }

    /// Choice options in `/Opt` order, identified by their position.
    fn choice_options(&self) -> Result<Vec<ChoiceOption>, SourceAdoptionError> {
        self.options
            .iter()
            .flatten()
            .enumerate()
            .map(|(index, option)| {
                Ok(ChoiceOption {
                    id: OptionId::try_from(index)?,
                    label: decode_text(Some(&option.display_value))?,
                    export_value: decode_text(Some(&option.export_value))?,
                })
            })
            .collect()
    }

    /// Identities of the source-selected options, which must all exist.
    fn selected_options(
        &self,
        options: &[ChoiceOption],
    ) -> Result<Vec<OptionId>, SourceAdoptionError> {
        self.selected_option_indices()?
            .into_iter()
            .map(|index| {
                let index =
                    usize::try_from(index).map_err(|_| SourceAdoptionError::IndexOverflow {
                        field: "option index",
                    })?;
                options.get(index).map(|option| option.id).ok_or(
                    SourceAdoptionError::InvalidSourceField {
                        reason: "choice option index",
                    },
                )
            })
            .collect()
    }

    fn list_box(&self, options: Vec<ChoiceOption>, selected: Vec<OptionId>) -> FieldKind {
        let selection = if self.is_multi_select() {
            ListSelection::Multiple(selected)
        } else {
            ListSelection::Single(selected.first().copied())
        };
        FieldKind::ListBox(ListBoxField { options, selection })
    }

    /// Combo whose value is a known option, custom text, or nothing.
    fn combo_box(
        &self,
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
            editable: self.is_editable(),
            value,
        })
    }
}

/// Options discovered so far for a toggle group and which one is currently on.
#[derive(Default)]
struct ToggleOptions<'a> {
    known: HashMap<&'a [u8], OptionId>,
    options: Vec<ChoiceOption>,
    selected: Option<OptionId>,
}

impl<'a> ToggleOptions<'a> {
    /// The option for `state`, reusing an earlier one only when `share_states` allows it.
    fn option_for(
        &mut self,
        state: &'a [u8],
        share_states: bool,
    ) -> Result<OptionId, SourceAdoptionError> {
        if share_states && let Some(option) = self.known.get(state) {
            return Ok(*option);
        }
        let option = OptionId::try_from(self.options.len())?;
        let label = decode_text(Some(state))?;
        self.known.insert(state, option);
        self.options.push(ChoiceOption {
            id: option,
            export_value: label.clone(),
            label,
        });
        Ok(option)
    }

    /// Records `option` as the group's on-state; a second distinct on-state is a conflict.
    fn mark_active(&mut self, option: OptionId) -> Result<(), SourceAdoptionError> {
        if self.selected.is_some_and(|previous| previous != option) {
            return Err(SourceAdoptionError::InvalidSourceField {
                reason: "conflicting button appearance states",
            });
        }
        self.selected = Some(option);
        Ok(())
    }
}

/// Option identity for a position in the option list.
impl TryFrom<usize> for OptionId {
    type Error = SourceAdoptionError;

    fn try_from(index: usize) -> Result<Self, Self::Error> {
        u32::try_from(index)
            .map(Self)
            .map_err(|_| SourceAdoptionError::IndexOverflow {
                field: "option index",
            })
    }
}
