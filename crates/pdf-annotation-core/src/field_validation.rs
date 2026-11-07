//! Structural rules for shared fields, their values, and the widgets bound to them.
//!
//! Completeness of required values is a separate submission concern, not checked here.
use std::collections::HashSet;

use crate::error::{FieldValidationError, ValidationError};
use crate::fields::{
    CheckboxGroupField, ChoiceOption, ComboBoxField, ComboValue, Field, FieldKind, ListBoxField,
    ListSelection, NewField, OptionId, RadioField, TextField, TextMode,
};
use crate::validate::Validate;
use crate::widgets::WidgetBinding;

/// A field kind whose value is checked against its fixed configuration.
pub(crate) trait FieldValue {
    /// Value a command may set.
    type Value: ?Sized;
    /// Fewest options the configuration allows.
    const MIN_OPTIONS: usize = 0;

    /// Options the value chooses from; free-text fields have none.
    fn options(&self) -> &[ChoiceOption] {
        &[]
    }

    /// Current value.
    fn value(&self) -> &Self::Value;

    /// Check `value` against the configuration, which is already valid.
    fn validate_value(&self, value: &Self::Value) -> Result<(), FieldValidationError>;

    /// Position of option `id`.
    fn option_position(&self, id: OptionId) -> Result<usize, FieldValidationError> {
        self.options()
            .iter()
            .position(|option| option.id == id)
            .ok_or(FieldValidationError::UnknownOption(id))
    }

    /// An absent selection is valid; a present one must name an option.
    fn validate_selected(&self, selected: Option<OptionId>) -> Result<(), FieldValidationError> {
        if let Some(id) = selected {
            self.option_position(id)?;
        }
        Ok(())
    }
}

/// Enough unique options, and a current value the configuration accepts.
impl<T: FieldValue> Validate for T {
    type Error = FieldValidationError;

    fn validate(&self) -> Result<(), FieldValidationError> {
        if self.options().len() < T::MIN_OPTIONS {
            return Err(FieldValidationError::EmptyOptions);
        }
        self.options().validate()?;
        self.validate_value(self.value())
    }
}

/// The fixed configuration with its initial value.
impl Validate for NewField {
    type Error = FieldValidationError;

    fn validate(&self) -> Result<(), FieldValidationError> {
        self.content.validate()
    }
}

impl Validate for FieldKind {
    type Error = FieldValidationError;

    fn validate(&self) -> Result<(), FieldValidationError> {
        match self {
            Self::Text(v) => v.validate(),
            Self::CheckboxGroup(v) => v.validate(),
            Self::Radio(v) => v.validate(),
            Self::ListBox(v) => v.validate(),
            Self::ComboBox(v) => v.validate(),
            Self::Checkbox(_) | Self::Button(_) | Self::Unsupported(_) => Ok(()),
        }
    }
}

/// Option identities must be unique within one field.
impl Validate for [ChoiceOption] {
    type Error = FieldValidationError;

    fn validate(&self) -> Result<(), FieldValidationError> {
        let mut seen = HashSet::with_capacity(self.len());
        match self.iter().find(|option| !seen.insert(option.id.0)) {
            Some(option) => Err(FieldValidationError::DuplicateOption(option.id)),
            None => Ok(()),
        }
    }
}

/// Text must fit the maximum length, and only multiline fields accept line breaks.
impl FieldValue for TextField {
    type Value = str;

    fn value(&self) -> &str {
        &self.value
    }

    fn validate_value(&self, value: &str) -> Result<(), FieldValidationError> {
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

impl FieldValue for CheckboxGroupField {
    type Value = Option<OptionId>;
    const MIN_OPTIONS: usize = 1;

    fn options(&self) -> &[ChoiceOption] {
        &self.options
    }

    fn value(&self) -> &Option<OptionId> {
        &self.selected
    }

    fn validate_value(&self, value: &Option<OptionId>) -> Result<(), FieldValidationError> {
        self.validate_selected(*value)
    }
}

impl FieldValue for RadioField {
    type Value = Option<OptionId>;
    const MIN_OPTIONS: usize = 1;

    fn options(&self) -> &[ChoiceOption] {
        &self.options
    }

    fn value(&self) -> &Option<OptionId> {
        &self.selected
    }

    fn validate_value(&self, value: &Option<OptionId>) -> Result<(), FieldValidationError> {
        self.validate_selected(*value)
    }
}

/// The selection keeps the field's cardinality; multiple selections name distinct
/// options in option-list order. Input is rejected rather than normalized.
impl FieldValue for ListBoxField {
    type Value = ListSelection;

    fn options(&self) -> &[ChoiceOption] {
        &self.options
    }

    fn value(&self) -> &ListSelection {
        &self.selection
    }

    fn validate_value(&self, value: &ListSelection) -> Result<(), FieldValidationError> {
        if std::mem::discriminant(&self.selection) != std::mem::discriminant(value) {
            return Err(FieldValidationError::SelectionModeMismatch);
        }
        match value {
            ListSelection::Single(selected) => self.validate_selected(*selected),
            ListSelection::Multiple(selected) => {
                // `None` orders before every index, so the first option always passes.
                let mut previous = None;
                for id in selected {
                    let index = Some(self.option_position(*id)?);
                    if index <= previous {
                        return Err(FieldValidationError::InvalidListSelection);
                    }
                    previous = index;
                }
                Ok(())
            }
        }
    }
}

/// Options must exist; custom text requires an editable combo and one line.
impl FieldValue for ComboBoxField {
    type Value = ComboValue;

    fn options(&self) -> &[ChoiceOption] {
        &self.options
    }

    fn value(&self) -> &ComboValue {
        &self.value
    }

    fn validate_value(&self, value: &ComboValue) -> Result<(), FieldValidationError> {
        match value {
            ComboValue::Option(id) => self.validate_selected(Some(*id)),
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
    pub(crate) fn check_target(self, field: &Field) -> Result<(), ValidationError> {
        let valid = match (self, &field.definition.content) {
            (Self::CheckboxOption { option, .. }, FieldKind::CheckboxGroup(group)) => {
                group.option_position(option).is_ok()
            }
            (Self::RadioOption { option, .. }, FieldKind::Radio(radio)) => {
                radio.option_position(option).is_ok()
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
