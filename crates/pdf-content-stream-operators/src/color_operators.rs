use std::sync::Arc;

use crate::{OperandContext, error::PdfOperatorError};
use pdf_object_reader::{ReadResult, pdf_string::PdfString};

/// The operands of a color-setting operator whose color space takes any number
/// of components (`SC`, `SCN`, `sc`, `scn`).
#[derive(Debug, Clone, PartialEq)]
pub struct ColorOperands {
    /// Color component values.
    pub components: Vec<f32>,
    /// An optional name of a pattern.
    pub pattern: Option<Arc<[u8]>>,
}

impl ColorOperands {
    /// Reads the operands of `SCN` or `scn`: the color components, optionally
    /// followed by a pattern name.
    #[inline]
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let mut components = Vec::with_capacity(operands.array().len());
        // The pattern name should come last, after the numeric color components
        for (index, object) in operands.array().iter().enumerate() {
            if object.is_name() {
                return Ok(Self {
                    components,
                    pattern: Some(operands.at::<PdfString>(index)?.bytes),
                });
            }
            components.push(operands.at(index)?);
        }

        Ok(Self {
            components,
            pattern: None,
        })
    }

    /// Reads the operands of `operator` (`SC` or `sc`), which takes at least one
    /// color component and no pattern.
    #[inline]
    pub(crate) fn read_without_pattern(
        operands: &mut OperandContext<'_, '_>,
        operator: &'static str,
    ) -> ReadResult<Self> {
        let components: Vec<f32> = operands.read_all()?;

        if components.is_empty() {
            return Err(PdfOperatorError::OperandCountMismatch {
                operator: operator.to_string(),
                expected: 1,
                actual: 0,
            }
            .into());
        }

        Ok(Self {
            components,
            pattern: None,
        })
    }
}
