use crate::{
    OperandContext,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
};
use pdf_object_reader::ReadResult;

/// Begins a new subpath by moving the current point to coordinates (x, y), omitting any connecting line segment.
/// If the `m` operator is the first operator in a path, it sets the current point.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveTo {
    /// The x-coordinate of the new current point.
    x: f32,
    /// The y-coordinate of the new current point.
    y: f32,
}

impl MoveTo {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [x, y] = operands.read_array()?;
        Ok(Self::new(x, y))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.move_to(self.x, self.y)
    }
}

/// Appends a straight line segment from the current point to the specified point (x, y).
/// The new current point becomes (x, y).
#[derive(Debug, Clone, PartialEq)]
pub struct LineTo {
    /// The x-coordinate of the line segment's end point.
    x: f32,
    /// The y-coordinate of the line segment's end point.
    y: f32,
}

impl LineTo {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [x, y] = operands.read_array()?;
        Ok(Self::new(x, y))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.line_to(self.x, self.y)
    }
}

/// Appends a cubic Bézier curve to the current path.
/// The curve extends from the current point to (x3, y3), using (x1, y1) and (x2, y2) as Bézier control points.
/// The new current point becomes (x3, y3).
#[derive(Debug, Clone, PartialEq)]
pub struct CurveTo {
    /// The x-coordinate of the first Bézier control point.
    x1: f32,
    /// The y-coordinate of the first Bézier control point.
    y1: f32,
    /// The x-coordinate of the second Bézier control point.
    x2: f32,
    /// The y-coordinate of the second Bézier control point.
    y2: f32,
    /// The x-coordinate of the curve's end point.
    x3: f32,
    /// The y-coordinate of the curve's end point.
    y3: f32,
}

impl CurveTo {
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            x3,
            y3,
        }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [x1, y1, x2, y2, x3, y3] = operands.read_array()?;
        Ok(Self::new(x1, y1, x2, y2, x3, y3))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.curve_to(self.x1, self.y1, self.x2, self.y2, self.x3, self.y3)
    }
}

/// Appends a cubic Bézier curve to the current path.
/// The current point is used as the first control point (x1, y1).
/// (x2, y2) is the second Bézier control point, and (x3, y3) is the end point of the curve.
/// The new current point becomes (x3, y3).
#[derive(Debug, Clone, PartialEq)]
pub struct CurveToV {
    /// The x-coordinate of the second Bézier control point.
    x2: f32,
    /// The y-coordinate of the second Bézier control point.
    y2: f32,
    /// The x-coordinate of the curve's end point.
    x3: f32,
    /// The y-coordinate of the curve's end point.
    y3: f32,
}

impl CurveToV {
    pub fn new(x2: f32, y2: f32, x3: f32, y3: f32) -> Self {
        Self { x2, y2, x3, y3 }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [x2, y2, x3, y3] = operands.read_array()?;
        Ok(Self::new(x2, y2, x3, y3))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.curve_to_v(self.x2, self.y2, self.x3, self.y3)
    }
}

/// Appends a cubic Bézier curve to the current path.
/// (x1, y1) is the first Bézier control point. The second control point (x2, y2) is the same as the curve's end point (x3, y3).
/// The new current point becomes (x3, y3).
#[derive(Debug, Clone, PartialEq)]
pub struct CurveToY {
    /// The x-coordinate of the first Bézier control point.
    x1: f32,
    /// The y-coordinate of the first Bézier control point.
    y1: f32,
    /// The x-coordinate of the curve's end point (and second control point).
    x3: f32,
    /// The y-coordinate of the curve's end point (and second control point).
    y3: f32,
}

impl CurveToY {
    pub fn new(x1: f32, y1: f32, x3: f32, y3: f32) -> Self {
        Self { x1, y1, x3, y3 }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [x1, y1, x3, y3] = operands.read_array()?;
        Ok(Self::new(x1, y1, x3, y3))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.curve_to_y(self.x1, self.y1, self.x3, self.y3)
    }
}

/// Appends a complete rectangle, defined by its bottom-left corner (x, y), width, and height,
/// to the current path as a complete subpath.
/// The new current point is undefined after this operation.
#[derive(Debug, Clone, PartialEq)]
pub struct Rectangle {
    /// The x-coordinate of the bottom-left corner of the rectangle.
    x: f32,
    /// The y-coordinate of the bottom-left corner of the rectangle.
    y: f32,
    /// The width of the rectangle.
    width: f32,
    /// The height of the rectangle.
    height: f32,
}

impl Rectangle {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [x, y, width, height] = operands.read_array()?;
        Ok(Self::new(x, y, width, height))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.rectangle(self.x, self.y, self.width, self.height)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::recording_pdf_operator_backend::RecordingBackend;

    use super::*;

    #[test]
    fn test_path_operations() {
        let mut backend = RecordingBackend::default();
        let a = MoveTo::new(0.0, 0.0);
        a.call(&mut backend).unwrap();
    }
}
