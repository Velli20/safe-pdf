//! Shared field definitions and values, independent of page widget placement.

use serde::{Deserialize, Serialize};

use crate::error::FieldValidationError;

/// Core-allocated document-local identity, distinct from an annotation identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct FieldId(
    #[serde(with = "crate::wire")]
    #[cfg_attr(feature = "typescript", ts(type = "string"))]
    pub u64,
);

/// Field-local option identity, independent of its display order or export value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct OptionId(pub u32);

/// An option whose identity remains distinct even when export values coincide.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct ChoiceOption {
    /// Caller-supplied identity, unique within this field's option collection.
    pub id: OptionId,
    /// User-facing option label.
    pub label: String,
    /// Value for a future form exporter; Core selection uses the identity.
    pub export_value: String,
}

/// Native text control behavior; password masking does not encrypt stored text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(rename_all = "snake_case")]
pub enum TextMode {
    /// Input rejects carriage returns and line feeds.
    SingleLine,
    /// Input permits line breaks.
    Multiline,
    /// Single-line input displayed with host-provided password masking.
    Password,
}

/// Shared Unicode text and its fixed input constraints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct TextField {
    /// Presentation and newline behavior.
    pub mode: TextMode,
    /// Optional maximum number of Unicode scalar values, not bytes or graphemes.
    pub max_length: Option<u32>,
    /// Current text; empty text is a valid draft even for a required field.
    pub value: String,
}

/// Shared boolean value displayed by every checkbox bound to the field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct CheckboxField {
    /// Whether the field is checked; required unchecked fields are incomplete.
    pub checked: bool,
}

/// Push button that triggers a host-owned action and never holds a value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct ButtonField {
    /// Unicode caption shown on the button.
    pub caption: String,
}

/// Checkbox views whose export states share one exclusive field value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct CheckboxGroupField {
    /// Nonempty options in document order; repeated views share an option identity.
    pub options: Vec<ChoiceOption>,
    /// Selected export state, or an unchecked draft.
    pub selected: Option<OptionId>,
}

impl CheckboxGroupField {
    pub(crate) fn validate_selection(
        &self,
        selected: Option<OptionId>,
    ) -> Result<(), FieldValidationError> {
        if let Some(id) = selected {
            option_position(&self.options, id)?;
        }
        Ok(())
    }
}

/// Exclusive option selection with optional repeated views of each option.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct RadioField {
    /// Nonempty options with distinct identities; export values may repeat.
    pub options: Vec<ChoiceOption>,
    /// Selected option, or an incomplete/unselected draft.
    pub selected: Option<OptionId>,
    /// Whether a value command may clear an existing selection.
    /// Initial and loaded unselected drafts remain valid when this is false.
    pub allow_clear: bool,
}

/// Listbox cardinality encoded together with its current selection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(tag = "mode", content = "selected", rename_all = "snake_case")]
pub enum ListSelection {
    /// At most one option is selected.
    Single(Option<OptionId>),
    /// Distinct option identities in option-list order; empty is allowed.
    Multiple(Vec<OptionId>),
}

/// A visible choice list with fixed options and selection cardinality.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct ListBoxField {
    /// Options in presentation order, each with a unique identity; may be empty.
    pub options: Vec<ChoiceOption>,
    /// Current selection; commands must preserve the single/multiple variant.
    pub selection: ListSelection,
}

/// Combo value distinguishes no selection, a known option, and arbitrary text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ComboValue {
    /// No value is selected or entered.
    Empty,
    /// Identity of an option in the field's option collection.
    Option(OptionId),
    /// Single-line text, allowed only for editable combos; empty text is a draft.
    Text(String),
}

/// A dropdown with optional arbitrary text entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct ComboBoxField {
    /// Options in presentation order, each with a unique identity; may be empty.
    pub options: Vec<ChoiceOption>,
    /// Whether custom text values are accepted.
    pub editable: bool,
    /// Current selection or custom text.
    pub value: ComboValue,
}

/// Typed field configuration and value; field kinds cannot change through edits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
#[non_exhaustive]
pub enum FieldKind {
    /// Single-line, multiline, or password text input.
    Text(TextField),
    /// Shared checked state.
    Checkbox(CheckboxField),
    /// Checkbox group with distinct export states and repeated views.
    CheckboxGroup(CheckboxGroupField),
    /// Exclusive selection among radio options.
    Radio(RadioField),
    /// Single- or multiple-selection listbox.
    ListBox(ListBoxField),
    /// Closed or editable dropdown.
    ComboBox(ComboBoxField),
    /// Push button; value commands are rejected and it is never incomplete.
    Button(ButtonField),
    /// Retained PDF field with no supported normalized value editor.
    Unsupported(Box<crate::metadata::UnsupportedField>),
}

/// Field creation data without the identity that Core will allocate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct NewField {
    /// Historical source properties; never a second live field value.
    #[serde(default)]
    pub metadata: crate::metadata::FieldMetadata,
    /// Host-facing field name; names need not be unique and are not identities.
    pub name: String,
    /// Accessible label for native controls.
    pub label: String,
    /// Reject value edits while allowing separate structural authoring operations.
    pub read_only: bool,
    /// Include an issue in completeness checks when the value is unfilled.
    pub required: bool,
    /// Fixed configuration and initial value to validate at creation.
    pub content: FieldKind,
}

/// Shared document field stored once regardless of how many widgets display it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Field {
    /// Stable, never-recycled identity within the document.
    pub id: FieldId,
    /// Field metadata, configuration, and current value.
    pub definition: NewField,
}

/// Nonfatal incompleteness reported for host submission, never a save error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct FieldIssue {
    /// Field whose required value is missing.
    pub field: FieldId,
    /// Machine-readable reason for presentation by the host.
    pub kind: FieldIssueKind,
}

/// Completeness problems distinct from structural validation failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FieldIssueKind {
    /// Required text is empty, checkbox unchecked, or choice unselected.
    RequiredValueMissing,
}

impl FieldKind {
    pub(crate) fn validate(&self) -> Result<(), crate::error::FieldValidationError> {
        use crate::error::FieldValidationError as Error;
        let options = match self {
            Self::Text(text) => return text.validate_value(&text.value),
            Self::Checkbox(_) | Self::Button(_) | Self::Unsupported(_) => return Ok(()),
            Self::CheckboxGroup(group) => {
                if group.options.is_empty() {
                    return Err(Error::EmptyCheckboxOptions);
                }
                &group.options
            }
            Self::Radio(radio) => {
                if radio.options.is_empty() {
                    return Err(Error::EmptyRadioOptions);
                }
                &radio.options
            }
            Self::ListBox(list) => &list.options,
            Self::ComboBox(combo) => &combo.options,
        };
        let mut seen = std::collections::HashSet::new();
        for option in options {
            if !seen.insert(option.id.0) {
                return Err(Error::DuplicateOption(option.id));
            }
        }
        match self {
            Self::CheckboxGroup(group) => group.validate_selection(group.selected)?,
            Self::Radio(radio) => radio.validate_selection(radio.selected)?,
            Self::ListBox(list) => list.validate_selection(&list.selection)?,
            Self::ComboBox(combo) => combo.validate_value(&combo.value)?,
            Self::Text(_) | Self::Checkbox(_) | Self::Button(_) | Self::Unsupported(_) => {}
        }
        Ok(())
    }

    pub(crate) fn is_empty(&self) -> bool {
        // Required values are submission issues, not errors while editing drafts.
        match self {
            Self::Text(text) => text.value.is_empty(),
            Self::Checkbox(checkbox) => !checkbox.checked,
            Self::Button(_) => false,
            Self::CheckboxGroup(group) => group.selected.is_none(),
            Self::Unsupported(field) => match &field.value {
                None | Some(crate::pdf_data::WidgetFieldValue::Null) => true,
                Some(crate::pdf_data::WidgetFieldValue::Bytes(value)) => value.is_empty(),
                Some(crate::pdf_data::WidgetFieldValue::Array(value)) => value.is_empty(),
                Some(crate::pdf_data::WidgetFieldValue::Dictionary) => false,
            },
            Self::Radio(radio) => radio.selected.is_none(),
            Self::ListBox(list) => match &list.selection {
                ListSelection::Single(value) => value.is_none(),
                ListSelection::Multiple(values) => values.is_empty(),
            },
            Self::ComboBox(combo) => match &combo.value {
                ComboValue::Empty => true,
                ComboValue::Option(_) => false,
                ComboValue::Text(text) => text.is_empty(),
            },
        }
    }
}

impl TextField {
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

impl RadioField {
    pub(crate) fn validate_selection(
        &self,
        selected: Option<OptionId>,
    ) -> Result<(), FieldValidationError> {
        if let Some(id) = selected {
            option_position(&self.options, id)?;
        }
        Ok(())
    }
}

impl ListBoxField {
    pub(crate) fn validate_selection(
        &self,
        selection: &ListSelection,
    ) -> Result<(), FieldValidationError> {
        if std::mem::discriminant(&self.selection) != std::mem::discriminant(selection) {
            return Err(FieldValidationError::SelectionModeMismatch);
        }
        match selection {
            ListSelection::Single(selected) => {
                if let Some(id) = selected {
                    option_position(&self.options, *id)?;
                }
            }
            ListSelection::Multiple(selected) => {
                // Unlike the legacy editor, Core rejects rather than normalizes input.
                let mut previous = None;
                for id in selected {
                    let index = option_position(&self.options, *id)?;
                    if previous.is_some_and(|previous| previous >= index) {
                        return Err(FieldValidationError::InvalidListSelection);
                    }
                    previous = Some(index);
                }
            }
        }
        Ok(())
    }
}

impl ComboBoxField {
    pub(crate) fn validate_value(&self, value: &ComboValue) -> Result<(), FieldValidationError> {
        match value {
            ComboValue::Option(id) => {
                option_position(&self.options, *id)?;
            }
            ComboValue::Text(text) => {
                if !self.editable {
                    return Err(FieldValidationError::CustomTextNotAllowed);
                }
                if text.contains(['\r', '\n']) {
                    return Err(FieldValidationError::LineBreakNotAllowed);
                }
            }
            ComboValue::Empty => {}
        }
        Ok(())
    }
}

fn option_position(options: &[ChoiceOption], id: OptionId) -> Result<usize, FieldValidationError> {
    options
        .iter()
        .position(|option| option.id == id)
        .ok_or(FieldValidationError::UnknownOption(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::FieldValidationError;

    fn options() -> Vec<ChoiceOption> {
        [OptionId(9), OptionId(1)]
            .into_iter()
            .map(|id| ChoiceOption {
                id,
                label: String::new(),
                export_value: String::new(),
            })
            .collect()
    }

    #[test]
    fn text_modes_count_scalars_and_only_multiline_accepts_newlines() {
        for mode in [
            TextMode::SingleLine,
            TextMode::Multiline,
            TextMode::Password,
        ] {
            for value in ["é文", "e\u{301}", "😀", ""] {
                assert!(
                    FieldKind::Text(TextField {
                        mode,
                        max_length: Some(2),
                        value: value.into()
                    })
                    .validate()
                    .is_ok()
                );
            }
            for value in ["\r", "\n", "\r\n"] {
                let result = FieldKind::Text(TextField {
                    mode,
                    max_length: None,
                    value: value.into(),
                })
                .validate();
                if mode == TextMode::Multiline {
                    assert!(result.is_ok());
                } else {
                    assert!(matches!(
                        result,
                        Err(FieldValidationError::LineBreakNotAllowed)
                    ));
                }
            }
        }
        assert!(matches!(
            FieldKind::Text(TextField {
                mode: TextMode::SingleLine,
                max_length: Some(0),
                value: "a".into()
            })
            .validate(),
            Err(FieldValidationError::TextTooLong { max: 0 })
        ));
    }

    #[test]
    fn choice_definitions_reject_duplicate_and_unknown_ids() {
        let mut duplicate = options();
        duplicate.push(duplicate.first().unwrap().clone());
        for content in [
            FieldKind::Radio(RadioField {
                options: duplicate.clone(),
                selected: None,
                allow_clear: true,
            }),
            FieldKind::ListBox(ListBoxField {
                options: duplicate.clone(),
                selection: ListSelection::Single(None),
            }),
            FieldKind::ComboBox(ComboBoxField {
                options: duplicate,
                editable: true,
                value: ComboValue::Empty,
            }),
        ] {
            assert!(matches!(
                content.validate(),
                Err(FieldValidationError::DuplicateOption(OptionId(9)))
            ));
        }
        for content in [
            FieldKind::Radio(RadioField {
                options: options(),
                selected: Some(OptionId(2)),
                allow_clear: true,
            }),
            FieldKind::ListBox(ListBoxField {
                options: options(),
                selection: ListSelection::Single(Some(OptionId(2))),
            }),
            FieldKind::ListBox(ListBoxField {
                options: options(),
                selection: ListSelection::Multiple(vec![OptionId(2)]),
            }),
            FieldKind::ComboBox(ComboBoxField {
                options: options(),
                editable: true,
                value: ComboValue::Option(OptionId(2)),
            }),
        ] {
            assert!(matches!(
                content.validate(),
                Err(FieldValidationError::UnknownOption(OptionId(2)))
            ));
        }
        assert!(matches!(
            FieldKind::Radio(RadioField {
                options: Vec::new(),
                selected: None,
                allow_clear: true
            })
            .validate(),
            Err(FieldValidationError::EmptyRadioOptions)
        ));
        for value in ["\r", "\n"] {
            assert!(matches!(
                FieldKind::ComboBox(ComboBoxField {
                    options: Vec::new(),
                    editable: true,
                    value: ComboValue::Text(value.into())
                })
                .validate(),
                Err(FieldValidationError::LineBreakNotAllowed)
            ));
        }
    }

    #[test]
    fn completeness_uses_values_not_whitespace_or_export_values() {
        let cases = [
            (
                FieldKind::Text(TextField {
                    mode: TextMode::Multiline,
                    max_length: None,
                    value: " \n".into(),
                }),
                false,
            ),
            (FieldKind::Checkbox(CheckboxField { checked: false }), true),
            (FieldKind::Checkbox(CheckboxField { checked: true }), false),
            (
                FieldKind::Radio(RadioField {
                    options: options(),
                    selected: None,
                    allow_clear: false,
                }),
                true,
            ),
            (
                FieldKind::Radio(RadioField {
                    options: options(),
                    selected: Some(OptionId(9)),
                    allow_clear: false,
                }),
                false,
            ),
            (
                FieldKind::ListBox(ListBoxField {
                    options: options(),
                    selection: ListSelection::Single(None),
                }),
                true,
            ),
            (
                FieldKind::ListBox(ListBoxField {
                    options: options(),
                    selection: ListSelection::Single(Some(OptionId(9))),
                }),
                false,
            ),
            (
                FieldKind::ListBox(ListBoxField {
                    options: Vec::new(),
                    selection: ListSelection::Multiple(Vec::new()),
                }),
                true,
            ),
            (
                FieldKind::ComboBox(ComboBoxField {
                    options: options(),
                    editable: true,
                    value: ComboValue::Text(String::new()),
                }),
                true,
            ),
            (
                FieldKind::ComboBox(ComboBoxField {
                    options: options(),
                    editable: true,
                    value: ComboValue::Text(" ".into()),
                }),
                false,
            ),
            (
                FieldKind::ComboBox(ComboBoxField {
                    options: options(),
                    editable: false,
                    value: ComboValue::Option(OptionId(9)),
                }),
                false,
            ),
        ];
        for (content, empty) in cases {
            assert!(content.validate().is_ok());
            assert_eq!(content.is_empty(), empty);
        }
    }
}
