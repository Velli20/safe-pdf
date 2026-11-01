//! Structural rules for shared fields, their values, and the widgets bound to them.
//!
//! Completeness of required values is a separate submission concern, not checked here.
use std::collections::HashSet;

use crate::error::{FieldValidationError, ValidationError};
use crate::fields::{
    ChoiceOption, ComboBoxField, ComboValue, Field, FieldKind, ListBoxField, ListSelection,
    NewField, OptionId, TextField, TextMode,
};
use crate::widgets::WidgetBinding;

impl NewField {
    /// Validate the fixed configuration with its initial value; finiteness is checked
    /// separately by [`crate::finite_values::check`].
    pub(crate) fn validate(&self) -> Result<(), FieldValidationError> {
        self.content.validate()
    }
}

impl FieldKind {
    /// Options must be unique, checkbox groups and radios need at least one, and the
    /// current value must satisfy the configuration.
    pub(crate) fn validate(&self) -> Result<(), FieldValidationError> {
        match self {
            Self::Text(text) => text.validate_value(&text.value),
            Self::Checkbox(_) | Self::Button(_) | Self::Unsupported(_) => Ok(()),
            Self::CheckboxGroup(group) => {
                check_options(&group.options, true)?;
                check_selected(&group.options, group.selected)
            }
            Self::Radio(radio) => {
                check_options(&radio.options, true)?;
                check_selected(&radio.options, radio.selected)
            }
            Self::ListBox(list) => {
                check_options(&list.options, false)?;
                list.validate_selection(&list.selection)
            }
            Self::ComboBox(combo) => {
                check_options(&combo.options, false)?;
                combo.validate_value(&combo.value)
            }
        }
    }
}

impl TextField {
    /// Text must fit the maximum length, and only multiline fields accept line breaks.
    pub(crate) fn validate_value(&self, value: &str) -> Result<(), FieldValidationError> {
        if let Some(max) = self.max_length
            && u64::try_from(value.chars().count()).unwrap_or(u64::MAX) > u64::from(max)
        {
            return Err(FieldValidationError::TextTooLong { max });
        }
        if self.mode != TextMode::Multiline && value.contains(['\r', '\n']) {
            return Err(FieldValidationError::LineBreakNotAllowed);
        }
        Ok(())
    }
}

impl ListBoxField {
    /// The selection keeps the field's cardinality; multiple selections name distinct
    /// options in option-list order. Input is rejected rather than normalized.
    pub(crate) fn validate_selection(
        &self,
        selection: &ListSelection,
    ) -> Result<(), FieldValidationError> {
        if std::mem::discriminant(&self.selection) != std::mem::discriminant(selection) {
            return Err(FieldValidationError::SelectionModeMismatch);
        }
        match selection {
            ListSelection::Single(selected) => check_selected(&self.options, *selected),
            ListSelection::Multiple(selected) => {
                let mut previous = None;
                for id in selected {
                    let index = option_position(&self.options, *id)?;
                    if previous.is_some_and(|previous| previous >= index) {
                        return Err(FieldValidationError::InvalidListSelection);
                    }
                    previous = Some(index);
                }
                Ok(())
            }
        }
    }
}

impl ComboBoxField {
    /// Options must exist; custom text requires an editable combo and one line.
    pub(crate) fn validate_value(&self, value: &ComboValue) -> Result<(), FieldValidationError> {
        match value {
            ComboValue::Option(id) => check_selected(&self.options, Some(*id)),
            ComboValue::Text(_) if !self.editable => {
                Err(FieldValidationError::CustomTextNotAllowed)
            }
            ComboValue::Text(text) if text.contains(['\r', '\n']) => {
                Err(FieldValidationError::LineBreakNotAllowed)
            }
            ComboValue::Text(_) | ComboValue::Empty => Ok(()),
        }
    }
}

impl WidgetBinding {
    /// Option bindings must name an option of a matching checkbox group or radio field;
    /// plain bindings must target any other field kind.
    pub(crate) fn validate(self, field: &Field) -> Result<(), ValidationError> {
        let has = |options: &[ChoiceOption], id: OptionId| options.iter().any(|o| o.id == id);
        let valid = match (self, &field.definition.content) {
            (Self::CheckboxOption { option, .. }, FieldKind::CheckboxGroup(group)) => {
                has(&group.options, option)
            }
            (Self::RadioOption { option, .. }, FieldKind::Radio(radio)) => {
                has(&radio.options, option)
            }
            (Self::Field { .. }, FieldKind::Radio(_) | FieldKind::CheckboxGroup(_))
            | (Self::RadioOption { .. } | Self::CheckboxOption { .. }, _) => false,
            (Self::Field { .. }, _) => true,
        };
        if valid {
            Ok(())
        } else {
            Err(ValidationError::InvalidWidgetBinding(field.id))
        }
    }
}

/// An absent selection is valid; a present one must name an option.
pub(crate) fn check_selected(
    options: &[ChoiceOption],
    selected: Option<OptionId>,
) -> Result<(), FieldValidationError> {
    selected.map_or(Ok(()), |id| option_position(options, id).map(drop))
}

/// Option identities must be unique, and nonempty when `required`.
fn check_options(options: &[ChoiceOption], required: bool) -> Result<(), FieldValidationError> {
    if required && options.is_empty() {
        return Err(FieldValidationError::EmptyOptions);
    }
    let mut seen = HashSet::new();
    match options.iter().find(|option| !seen.insert(option.id.0)) {
        Some(option) => Err(FieldValidationError::DuplicateOption(option.id)),
        None => Ok(()),
    }
}

/// Position of option `id` in `options`.
fn option_position(options: &[ChoiceOption], id: OptionId) -> Result<usize, FieldValidationError> {
    options
        .iter()
        .position(|option| option.id == id)
        .ok_or(FieldValidationError::UnknownOption(id))
}
