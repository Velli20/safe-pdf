use std::sync::Arc;

use num_traits::FromPrimitive;
use pdf_graphics::{LineCap, LineJoin, transform::Transform};
use pdf_object_reader::{ReadResult, pdf_string::PdfString};

use crate::{
    OperandContext,
    error::PdfOperatorError,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
};

/// Sets the line width for path stroking.
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineWidth {
    /// The new line width in user space units.
    width: f32,
}

impl SetLineWidth {
    pub fn new(width: f32) -> Self {
        Self { width }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let width = operands.at(0)?;
        Ok(Self::new(width))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_line_width(self.width)
    }
}

/// Sets the line cap style for path stroking.
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineCapStyle {
    /// The line cap style to apply.
    style: LineCap,
}

impl SetLineCapStyle {
    pub fn new(style: u8) -> Result<Self, PdfOperatorError> {
        match LineCap::from_u8(style) {
            Some(style_enum) => Ok(Self { style: style_enum }),
            None => Err(PdfOperatorError::InvalidOperandValue {
                expected: "line cap style (0 butt, 1 round, 2 projecting square)",
                found: style.to_string(),
            }),
        }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let style = operands.at(0)?;
        Ok(Self::new(style)?)
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_line_cap(self.style)
    }
}

/// Sets the line join style for path stroking.
#[derive(Debug, Clone, PartialEq)]
pub struct SetLineJoinStyle {
    /// The line join style to apply.
    style: LineJoin,
}

impl SetLineJoinStyle {
    pub fn new(style: u8) -> Result<Self, PdfOperatorError> {
        match LineJoin::from_u8(style) {
            Some(style_enum) => Ok(Self { style: style_enum }),
            None => Err(PdfOperatorError::InvalidOperandValue {
                expected: "line join style (0 miter, 1 round, 2 bevel)",
                found: style.to_string(),
            }),
        }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let style = operands.at(0)?;
        Ok(Self::new(style)?)
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_line_join(self.style)
    }
}

/// Sets the miter limit for path stroking.
#[derive(Debug, Clone, PartialEq)]
pub struct SetMiterLimit {
    /// The new miter limit. This controls when a miter join is automatically
    /// converted to a bevel join to prevent excessively long spikes.
    limit: f32,
}

impl SetMiterLimit {
    pub fn new(limit: f32) -> Self {
        Self { limit }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let limit = operands.at(0)?;
        Ok(Self::new(limit))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_miter_limit(self.limit)
    }
}

/// Sets the dash pattern for path stroking.
#[derive(Debug, Clone, PartialEq)]
pub struct SetDashPattern {
    /// An array of numbers specifying the lengths of alternating dashes and gaps.
    array: Vec<f32>,
    /// The phase, specifying the distance into the dash pattern at which to start.
    phase: f32,
}

impl SetDashPattern {
    pub fn new(array: Vec<f32>, phase: f32) -> Self {
        Self { array, phase }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let array = operands.at(0)?;
        let phase = operands.at(1)?;
        Ok(Self::new(array, phase))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_dash_pattern(&self.array, self.phase)
    }
}

/// Sets the flatness tolerance, controlling the accuracy with which curves
/// are approximated. (PDF operator `i`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetFlatnessTolerance {
    /// The flatness tolerance value. Typical values range from 1 to 100.
    tolerance: f32,
}

impl SetFlatnessTolerance {
    pub fn new(tolerance: f32) -> Self {
        Self { tolerance }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let tolerance = operands.at(0)?;
        Ok(Self::new(tolerance))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_flatness_tolerance(self.tolerance)
    }
}

/// Sets the rendering intent for color reproduction. (PDF operator `ri`)
///
/// The operand is a name object indicating the intent, e.g.,
/// /AbsoluteColorimetric, /RelativeColorimetric, /Saturation, /Perceptual.
#[derive(Debug, Clone, PartialEq)]
pub struct SetRenderingIntent {
    intent: Arc<[u8]>,
}

impl SetRenderingIntent {
    pub fn new(intent: Arc<[u8]>) -> Self {
        Self { intent }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let intent = operands.at::<PdfString>(0)?.bytes;
        Ok(Self::new(intent))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_rendering_intent(&self.intent)
    }
}

/// Modifies the current transformation matrix (CTM) by concatenating the specified matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct ConcatMatrix {
    /// The matrix to concatenate with the CTM.
    matrix: Transform,
}

impl ConcatMatrix {
    pub fn new(matrix: [f32; 6]) -> Self {
        Self {
            matrix: Transform::from(matrix),
        }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        Ok(Self::new(operands.read_array()?))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.concat_matrix(&self.matrix)
    }
}

/// Sets multiple graphics state parameters from a named graphics state parameter dictionary.
/// The dictionary is expected to be in the resource dictionary. (PDF operator `gs`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetGraphicsStateFromDict {
    /// The name of the graphics state parameter dictionary.
    dict_name: Arc<[u8]>,
}

impl SetGraphicsStateFromDict {
    pub fn new(dict_name: Arc<[u8]>) -> Self {
        Self { dict_name }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let dict_name = operands.at::<PdfString>(0)?.bytes;
        Ok(Self::new(dict_name))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_graphics_state_from_dict(&self.dict_name)
    }
}
