//! Page-local views of shared fields; native controls and styling belong to hosts.

use serde::{Deserialize, Serialize};

use crate::fields::{FieldId, OptionId};
use pdf_graphics::rect::Rect;

/// Binding to shared state; a radio widget additionally names its option.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WidgetBinding {
    /// Display one export state of a checkbox group.
    CheckboxOption {
        /// Existing checkbox group.
        field: FieldId,
        /// Option shared by every view with this export state.
        option: OptionId,
    },
    /// Display a text, checkbox, listbox, or combo field; radio fields are invalid.
    Field {
        /// Existing document field whose kind determines the native control.
        field: FieldId,
    },
    /// Display one option of a radio field, sharing selection with its other views.
    RadioOption {
        /// Existing radio field.
        field: FieldId,
        /// Existing option in that field.
        option: OptionId,
    },
}

/// Annotation geometry and reference to shared field data, without a local value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Widget {
    /// Optional precise current control style, independent of historical PDF metadata.
    #[serde(default)]
    pub style: Option<crate::style::ResolvedStyle>,
    /// Placement on the containing annotation's page.
    /// Bounds, including their maximum corner, must remain finite after edits.
    pub bounds: Rect<f64>,
    /// Immutable binding; delete and create an annotation to change its field/option.
    pub binding: WidgetBinding,
}

impl WidgetBinding {
    /// Shared field this binding reads and writes, whatever its option.
    pub fn field(self) -> FieldId {
        match self {
            Self::Field { field }
            | Self::RadioOption { field, .. }
            | Self::CheckboxOption { field, .. } => field,
        }
    }

    pub(crate) fn validate(
        self,
        field: &crate::fields::Field,
    ) -> Result<(), crate::error::ValidationError> {
        use crate::fields::FieldKind;
        let valid = match (self, &field.definition.content) {
            (Self::CheckboxOption { option, .. }, FieldKind::CheckboxGroup(group)) => {
                group.options.iter().any(|candidate| candidate.id == option)
            }
            (Self::RadioOption { option, .. }, FieldKind::Radio(radio)) => {
                radio.options.iter().any(|candidate| candidate.id == option)
            }
            (Self::Field { .. }, FieldKind::Radio(_) | FieldKind::CheckboxGroup(_))
            | (Self::RadioOption { .. } | Self::CheckboxOption { .. }, _) => false,
            (Self::Field { .. }, _) => true,
        };
        if valid {
            Ok(())
        } else {
            Err(crate::error::ValidationError::InvalidWidgetBinding(
                field.id,
            ))
        }
    }
}
