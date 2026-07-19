use crate::{
    error::PdfOperatorError,
    operands::Operands,
    operator_trait::PdfOperator,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
};

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
}

impl PdfOperator for SetGrayFill {
    const NAME: &'static [u8] = b"g";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let gray = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetGrayFill(Self::new(gray)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
}

impl PdfOperator for SetGrayStroke {
    const NAME: &'static [u8] = b"G";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let gray = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetGrayStroke(Self::new(gray)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
}

impl PdfOperator for SetRGBFill {
    const NAME: &'static [u8] = b"rg";

    const OPERAND_COUNT: Option<usize> = Some(3);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let r = operands.get_f32()?;
        let g = operands.get_f32()?;
        let b = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetRGBFill(Self::new(r, g, b)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
}

impl PdfOperator for SetRGBStroke {
    const NAME: &'static [u8] = b"RG";

    const OPERAND_COUNT: Option<usize> = Some(3);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let r = operands.get_f32()?;
        let g = operands.get_f32()?;
        let b = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetRGBStroke(Self::new(r, g, b)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
}

impl PdfOperator for SetCMYKFill {
    const NAME: &'static [u8] = b"k";

    const OPERAND_COUNT: Option<usize> = Some(4);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let c = operands.get_f32()?;
        let m = operands.get_f32()?;
        let y = operands.get_f32()?;
        let k = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetCMYKFill(Self::new(c, m, y, k)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
}

impl PdfOperator for SetCMYKStroke {
    const NAME: &'static [u8] = b"K";

    const OPERAND_COUNT: Option<usize> = Some(4);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let c = operands.get_f32()?;
        let m = operands.get_f32()?;
        let y = operands.get_f32()?;
        let k = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetCMYKStroke(Self::new(c, m, y, k)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.set_stroking_cmyk(self.c, self.m, self.y, self.k)
    }
}

/// Sets the stroke color space value.
#[derive(Debug, Clone, PartialEq)]
pub struct SetStrokeColorSpace {
    /// The name of the color space.
    name: String,
}

impl SetStrokeColorSpace {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl PdfOperator for SetStrokeColorSpace {
    const NAME: &'static [u8] = b"CS";
    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let name = operands.get_str()?;
        Ok(PdfOperatorVariant::SetStrokeColorSpace(Self::new(name)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.set_stroking_color_space(&self.name)
    }
}

/// Sets the non-stroking (fill) color space value.
#[derive(Debug, Clone, PartialEq)]
pub struct SetNonStrokingColorSpace {
    /// The name of the color space.
    name: String,
}

impl SetNonStrokingColorSpace {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl PdfOperator for SetNonStrokingColorSpace {
    const NAME: &'static [u8] = b"cs";
    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let name = operands.get_str()?;
        Ok(PdfOperatorVariant::SetNonStrokingColorSpace(Self::new(
            name,
        )))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
    pattern: Option<String>,
}

impl SetStrokingColor {
    pub fn new(components: Vec<f32>, pattern: Option<String>) -> Self {
        Self {
            components,
            pattern,
        }
    }
}

impl PdfOperator for SetStrokingColor {
    const NAME: &'static [u8] = b"SCN";
    const OPERAND_COUNT: Option<usize> = None;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let mut values = vec![];
        while let Some(value) = operands.peek_next() {
            if value.is_name() {
                break;
            }
            let v = operands.get_f32()?;
            values.push(v);
        }

        // The pattern name should come last, after the numeric color components
        let pattern = if operands
            .peek_next()
            .map(|obj| obj.is_name())
            .unwrap_or(false)
        {
            operands.get_str().ok()
        } else {
            None
        };

        Ok(PdfOperatorVariant::SetStrokingColor(Self::new(
            values, pattern,
        )))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
    pattern: Option<String>,
}

impl SetNonStrokingColor {
    pub fn new(components: Vec<f32>, pattern: Option<String>) -> Self {
        Self {
            components,
            pattern,
        }
    }
}

impl PdfOperator for SetNonStrokingColor {
    const NAME: &'static [u8] = b"scn";
    const OPERAND_COUNT: Option<usize> = None;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let mut values = vec![];
        while let Some(value) = operands.peek_next() {
            if value.is_name() {
                break;
            }
            let v = operands.get_f32()?;
            values.push(v);
        }

        // The pattern name should come last, after the numeric color components
        let pattern = if operands
            .peek_next()
            .map(|obj| obj.is_name())
            .unwrap_or(false)
        {
            operands.get_str().ok()
        } else {
            None
        };

        Ok(PdfOperatorVariant::SetNonStrokingColor(Self::new(
            values, pattern,
        )))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        if let Some(pattern) = &self.pattern {
            backend.set_non_stroking_color_extended(&self.components, pattern)
        } else {
            backend.set_non_stroking_color(&self.components)
        }
    }
}

/// Sets the non-stroking color when the color space requires
/// multiple color components, without pattern support.
/// This handles the "sc" operator and maps to `SetNonStrokingColor` internally.
#[derive(Debug, Clone, PartialEq)]
pub struct SetNonStrokingColorSc;

impl PdfOperator for SetNonStrokingColorSc {
    const NAME: &'static [u8] = b"sc";
    const OPERAND_COUNT: Option<usize> = None;

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
        // Unreachable: read() always produces a SetNonStrokingColor variant,
        // so this type is never dispatched through PdfOperatorVariant::call.
        Ok(())
    }

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let mut values = vec![];
        while operands.peek_next().is_some() {
            values.push(operands.get_f32()?);
        }

        if values.is_empty() {
            return Err(PdfOperatorError::OperandCountMismatch {
                operator: "sc".to_string(),
                expected: 1,
                actual: 0,
            });
        }

        Ok(PdfOperatorVariant::SetNonStrokingColor(
            SetNonStrokingColor::new(values, None),
        ))
    }
}

/// Sets the stroking color when the color space requires
/// multiple color components, without pattern support.
/// This handles the "SC" operator and maps to `SetStrokingColor` internally.
#[derive(Debug, Clone, PartialEq)]
pub struct SetStrokingColorSc;

impl PdfOperator for SetStrokingColorSc {
    const NAME: &'static [u8] = b"SC";
    const OPERAND_COUNT: Option<usize> = None;

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
        // Unreachable: read() always produces a SetStrokingColor variant,
        // so this type is never dispatched through PdfOperatorVariant::call.
        Ok(())
    }

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let mut values = vec![];
        while operands.peek_next().is_some() {
            values.push(operands.get_f32()?);
        }

        if values.is_empty() {
            return Err(PdfOperatorError::OperandCountMismatch {
                operator: "SC".to_string(),
                expected: 1,
                actual: 0,
            });
        }

        Ok(PdfOperatorVariant::SetStrokingColor(SetStrokingColor::new(
            values, None,
        )))
    }
}
