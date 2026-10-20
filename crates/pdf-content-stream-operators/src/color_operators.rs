use std::sync::Arc;

use crate::{
    OperandContext,
    error::PdfOperatorError,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
};
use pdf_object_reader::{ReadResult, pdf_string::PdfString};

/// Sets the fill color to a grayscale value.
/// The gray level applies to subsequent fill operations.
#[derive(Debug, Clone, PartialEq)]
pub struct SetGrayFill {
    /// The gray level, a value between 0.0 (black) and 1.0 (white).
    gray: f32,
}

impl SetGrayFill {
    pub fn new(gray: f32) -> Self {
        Self { gray }
    }

    /// Returns the gray component.
    pub const fn gray(&self) -> f32 {
        self.gray
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let gray = operands.at(0)?;
        Ok(Self::new(gray))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_non_stroking_gray(self.gray)
    }
}

/// Sets the stroke color to a grayscale value.
/// The gray level applies to subsequent stroke operations.
#[derive(Debug, Clone, PartialEq)]
pub struct SetGrayStroke {
    /// The gray level, a value between 0.0 (black) and 1.0 (white).
    gray: f32,
}

impl SetGrayStroke {
    pub fn new(gray: f32) -> Self {
        Self { gray }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let gray = operands.at(0)?;
        Ok(Self::new(gray))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_stroking_gray(self.gray)
    }
}

/// Sets the fill color to an RGB (Red, Green, Blue) value.
/// The RGB color applies to subsequent fill operations.
#[derive(Debug, Clone, PartialEq)]
pub struct SetRGBFill {
    /// The red component, a value between 0.0 and 1.0.
    r: f32,
    /// The green component, a value between 0.0 and 1.0.
    g: f32,
    /// The blue component, a value between 0.0 and 1.0.
    b: f32,
}

impl SetRGBFill {
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Returns the RGB components.
    pub const fn components(&self) -> [f32; 3] {
        [self.r, self.g, self.b]
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [r, g, b] = operands.read_array()?;
        Ok(Self::new(r, g, b))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_non_stroking_rgb(self.r, self.g, self.b)
    }
}

/// Sets the stroke color to an RGB (Red, Green, Blue) value.
/// The RGB color applies to subsequent stroke operations.
#[derive(Debug, Clone, PartialEq)]
pub struct SetRGBStroke {
    /// The red component, a value between 0.0 and 1.0.
    r: f32,
    /// The green component, a value between 0.0 and 1.0.
    g: f32,
    /// The blue component, a value between 0.0 and 1.0.
    b: f32,
}

impl SetRGBStroke {
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [r, g, b] = operands.read_array()?;
        Ok(Self::new(r, g, b))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_stroking_rgb(self.r, self.g, self.b)
    }
}

/// Sets the fill color to a CMYK (Cyan, Magenta, Yellow, Black/Key) value.
/// The CMYK color applies to subsequent fill operations.
#[derive(Debug, Clone, PartialEq)]
pub struct SetCMYKFill {
    /// The cyan component, a value between 0.0 and 1.0.
    c: f32,
    /// The magenta component, a value between 0.0 and 1.0.
    m: f32,
    /// The yellow component, a value between 0.0 and 1.0.
    y: f32,
    /// The black (key) component, a value between 0.0 and 1.0.
    k: f32,
}

impl SetCMYKFill {
    pub fn new(c: f32, m: f32, y: f32, k: f32) -> Self {
        Self { c, m, y, k }
    }

    /// Returns the CMYK components.
    pub const fn components(&self) -> [f32; 4] {
        [self.c, self.m, self.y, self.k]
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [c, m, y, k] = operands.read_array()?;
        Ok(Self::new(c, m, y, k))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_non_stroking_cmyk(self.c, self.m, self.y, self.k)
    }
}

/// Sets the stroke color to a CMYK (Cyan, Magenta, Yellow, Black/Key) value.
/// The CMYK color applies to subsequent stroke operations.
#[derive(Debug, Clone, PartialEq)]
pub struct SetCMYKStroke {
    /// The cyan component, a value between 0.0 and 1.0.
    c: f32,
    /// The magenta component, a value between 0.0 and 1.0.
    m: f32,
    /// The yellow component, a value between 0.0 and 1.0.
    y: f32,
    /// The black (key) component, a value between 0.0 and 1.0.
    k: f32,
}

impl SetCMYKStroke {
    pub fn new(c: f32, m: f32, y: f32, k: f32) -> Self {
        Self { c, m, y, k }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [c, m, y, k] = operands.read_array()?;
        Ok(Self::new(c, m, y, k))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_stroking_cmyk(self.c, self.m, self.y, self.k)
    }
}

/// Sets the stroke color space value.
#[derive(Debug, Clone, PartialEq)]
pub struct SetStrokeColorSpace {
    /// The name of the color space.
    name: Arc<[u8]>,
}

impl SetStrokeColorSpace {
    pub fn new(name: Arc<[u8]>) -> Self {
        Self { name }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let name = operands.at::<PdfString>(0)?.bytes;
        Ok(Self::new(name))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_stroking_color_space(&self.name)
    }
}

/// Sets the non-stroking (fill) color space value.
#[derive(Debug, Clone, PartialEq)]
pub struct SetNonStrokingColorSpace {
    /// The name of the color space.
    name: Arc<[u8]>,
}

impl SetNonStrokingColorSpace {
    pub fn new(name: Arc<[u8]>) -> Self {
        Self { name }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let name = operands.at::<PdfString>(0)?.bytes;
        Ok(Self::new(name))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.set_non_stroking_color_space(&self.name)
    }
}

/// Sets the stroking color when the color space requires
/// multiple color components.
#[derive(Debug, Clone, PartialEq)]
pub struct SetStrokingColor {
    /// Color component values.
    components: Vec<f32>,
    /// An optional name of a pattern.
    pattern: Option<Arc<[u8]>>,
}

impl SetStrokingColor {
    pub fn new(components: Vec<f32>, pattern: Option<Arc<[u8]>>) -> Self {
        Self {
            components,
            pattern,
        }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        // The pattern name should come last, after the numeric color components
        let count = operands
            .array()
            .iter()
            .take_while(|value| !value.is_name())
            .count();
        let values = (0..count)
            .map(|index| operands.at(index))
            .collect::<ReadResult<_>>()?;
        let pattern = operands.at::<PdfString>(count).ok().map(|name| name.bytes);

        Ok(Self::new(values, pattern))
    }

    /// Reads the operator from the color components of `SC`, which takes no pattern.
    pub(crate) fn read_without_pattern(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let values: Vec<f32> = operands.read_all()?;

        if values.is_empty() {
            return Err(PdfOperatorError::OperandCountMismatch {
                operator: "SC".to_string(),
                expected: 1,
                actual: 0,
            }
            .into());
        }

        Ok(Self::new(values, None))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        if let Some(pattern) = &self.pattern {
            backend.set_stroking_color_extended(&self.components, pattern)
        } else {
            backend.set_stroking_color(&self.components)
        }
    }
}

/// Sets the non-stroking color when the color space requires
/// multiple color components.
#[derive(Debug, Clone, PartialEq)]
pub struct SetNonStrokingColor {
    /// Color component values.
    components: Vec<f32>,
    /// An optional name of a pattern.
    pattern: Option<Arc<[u8]>>,
}

impl SetNonStrokingColor {
    pub fn new(components: Vec<f32>, pattern: Option<Arc<[u8]>>) -> Self {
        Self {
            components,
            pattern,
        }
    }

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        // The pattern name should come last, after the numeric color components
        let count = operands
            .array()
            .iter()
            .take_while(|value| !value.is_name())
            .count();
        let values = (0..count)
            .map(|index| operands.at(index))
            .collect::<ReadResult<_>>()?;
        let pattern = operands.at::<PdfString>(count).ok().map(|name| name.bytes);

        Ok(Self::new(values, pattern))
    }

    /// Reads the operator from the color components of `sc`, which takes no pattern.
    pub(crate) fn read_without_pattern(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let values: Vec<f32> = operands.read_all()?;

        if values.is_empty() {
            return Err(PdfOperatorError::OperandCountMismatch {
                operator: "sc".to_string(),
                expected: 1,
                actual: 0,
            }
            .into());
        }

        Ok(Self::new(values, None))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        if let Some(pattern) = &self.pattern {
            backend.set_non_stroking_color_extended(&self.components, pattern)
        } else {
            backend.set_non_stroking_color(&self.components)
        }
    }
}
