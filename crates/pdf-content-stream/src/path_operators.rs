use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
    pdf_operator_backend::PdfOperatorBackend,
};

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
}

impl PdfOperator for MoveTo {
    const NAME: &'static str = "m";

    const OPERAND_COUNT: Option<usize> = Some(2);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let x = operands.get_f32()?;
        let y = operands.get_f32()?;
        Ok(PdfOperatorVariant::MoveTo(Self::new(x, y)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
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
}

impl PdfOperator for LineTo {
    const NAME: &'static str = "l";

    const OPERAND_COUNT: Option<usize> = Some(2);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let x = operands.get_f32()?;
        let y = operands.get_f32()?;
        Ok(PdfOperatorVariant::LineTo(Self::new(x, y)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
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
}

impl PdfOperator for CurveTo {
    const NAME: &'static str = "c";

    const OPERAND_COUNT: Option<usize> = Some(6);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let x1 = operands.get_f32()?;
        let y1 = operands.get_f32()?;
        let x2 = operands.get_f32()?;
        let y2 = operands.get_f32()?;
        let x3 = operands.get_f32()?;
        let y3 = operands.get_f32()?;
        Ok(PdfOperatorVariant::CurveTo(Self::new(
            x1, y1, x2, y2, x3, y3,
        )))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
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
}

impl PdfOperator for CurveToV {
    const NAME: &'static str = "v";

    const OPERAND_COUNT: Option<usize> = Some(4);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let x2 = operands.get_f32()?;
        let y2 = operands.get_f32()?;
        let x3 = operands.get_f32()?;
        let y3 = operands.get_f32()?;
        Ok(PdfOperatorVariant::CurveToV(Self::new(x2, y2, x3, y3)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
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
}

impl PdfOperator for CurveToY {
    const NAME: &'static str = "y";

    const OPERAND_COUNT: Option<usize> = Some(4);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let x1 = operands.get_f32()?;
        let y1 = operands.get_f32()?;
        let x3 = operands.get_f32()?;
        let y3 = operands.get_f32()?;
        Ok(PdfOperatorVariant::CurveToY(Self::new(x1, y1, x3, y3)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.curve_to_y(self.x1, self.y1, self.x3, self.y3)
    }
}

/// Closes the current subpath by appending a straight line segment from the current point
/// to the starting point of the subpath. (PDF operator `h`)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClosePath;

impl PdfOperator for ClosePath {
    const NAME: &'static str = "h";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::ClosePath(Self::default()))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.close_path()
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
}

impl PdfOperator for Rectangle {
    const NAME: &'static str = "re";

    const OPERAND_COUNT: Option<usize> = Some(4);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let x = operands.get_f32()?;
        let y = operands.get_f32()?;
        let width = operands.get_f32()?;
        let height = operands.get_f32()?;
        Ok(PdfOperatorVariant::Rectangle(Self::new(
            x, y, width, height,
        )))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.rectangle(self.x, self.y, self.width, self.height)
    }
}

#[cfg(test)]
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
