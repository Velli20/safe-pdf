//! `/VE` visibility expressions of optional content membership dictionaries.
use crate::error::OptionalContentError;
use crate::optional_content::{OptionalContentGroupId, OptionalContentState};
use pdf_object_reader::object_variant::ObjectVariant;
use pdf_object_reader::{ArrayContext, FromPdfObject, ObjectAccess, ObjectContext, ReadResult};
use std::collections::BTreeSet;
use std::sync::Arc;

/// An `/OCMD` `/VE` visibility expression combining group states with boolean operators.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub enum VisibilityExpression {
    /// The state of one group. A membership dictionary named as an operand is looked up
    /// as a group, never evaluated as a membership.
    Group(OptionalContentGroupId),
    /// `/Not`: the negation of its single operand.
    Not(Box<VisibilityExpression>),
    /// `/And`: true when every operand is.
    And(Vec<VisibilityExpression>),
    /// `/Or`: true when any operand is.
    Or(Vec<VisibilityExpression>),
}

impl VisibilityExpression {
    /// Whether any group this expression reads appears in `changed`.
    pub fn depends_on(&self, changed: &BTreeSet<OptionalContentGroupId>) -> bool {
        match self {
            Self::Group(id) => changed.contains(id),
            Self::Not(operand) => operand.depends_on(changed),
            Self::And(operands) | Self::Or(operands) => {
                operands.iter().any(|operand| operand.depends_on(changed))
            }
        }
    }

    /// Evaluates the expression against `state`. An `/And` or `/Or` without operands
    /// is false.
    pub(crate) fn is_visible(&self, state: &OptionalContentState) -> bool {
        match self {
            Self::Group(id) => state.group_visible(id),
            Self::Not(operand) => !operand.is_visible(state),
            Self::And(operands) => {
                !operands.is_empty() && operands.iter().all(|operand| operand.is_visible(state))
            }
            Self::Or(operands) => operands.iter().any(|operand| operand.is_visible(state)),
        }
    }
}

/// Decodes a `/VE` visibility expression: an operator array or a group dictionary.
impl FromPdfObject for VisibilityExpression {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        // Only an expression is an array; any other operand names an optional content group.
        if !matches!(context.object(), ObjectVariant::Array(_)) {
            return Ok(Self::Group(context.decode()?));
        }
        let mut context = context.array()?;
        // An expression is `[operator operand ...]`: the leading name selects the operator
        // and every element after it is one of its operands.
        let Some((operator, operands)) = context.array().split_first() else {
            return Err(OptionalContentError::InvalidEntry {
                entry: b"VE",
                reason: "empty visibility expression".to_owned(),
            }
            .into());
        };
        let operator = context.read::<Arc<[u8]>>(operator)?;
        let operands = ArrayContext::new(operands, context.access()).read_all::<Self>()?;
        match &*operator {
            b"And" => Ok(Self::And(operands)),
            b"Or" => Ok(Self::Or(operands)),
            // `/Not` negates exactly one operand.
            b"Not" => match <[Self; 1]>::try_from(operands) {
                Ok([operand]) => Ok(Self::Not(Box::new(operand))),
                Err(_) => Err(OptionalContentError::InvalidEntry {
                    entry: b"VE",
                    reason: "/Not takes exactly one operand".to_owned(),
                }
                .into()),
            },
            other => Err(OptionalContentError::InvalidEntry {
                entry: b"VE",
                reason: format!("unsupported visibility expression operator '{other:?}'"),
            }
            .into()),
        }
    }
}
