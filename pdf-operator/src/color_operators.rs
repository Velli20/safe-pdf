use crate::{
    error::PdfPainterError,
    pdf_operator::{Operands, PdfOperatorVariant},
};

/// Sets the fill color to a grayscale value. (PDF operator `g`)
/// The gray level applies to subsequent fill operations.
#[derive(Debug, Clone, PartialEq)]
pub struct SetGrayFill {
    /// The gray level, a value between 0.0 (black) and 1.0 (white).
    gray: f32,
}

impl SetGrayFill {
    pub const fn operator_name() -> &'static str {
        "g"
    }

    pub fn new(gray: f32) -> Self {
        Self { gray }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let gray = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetGrayFill(Self::new(gray)))
    }
}

/// Sets the stroke color to a grayscale value. (PDF operator `G`)
/// The gray level applies to subsequent stroke operations.
#[derive(Debug, Clone, PartialEq)]
pub struct SetGrayStroke {
    /// The gray level, a value between 0.0 (black) and 1.0 (white).
    gray: f32,
}

impl SetGrayStroke {
    pub const fn operator_name() -> &'static str {
        "G"
    }

    pub fn new(gray: f32) -> Self {
        Self { gray }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let gray = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetGrayStroke(Self::new(gray)))
    }
}

/// Sets the fill color to an RGB (Red, Green, Blue) value. (PDF operator `rg`)
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
    pub const fn operator_name() -> &'static str {
        "rg"
    }

    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let r = operands.get_f32()?;
        let g = operands.get_f32()?;
        let b = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetRGBFill(Self::new(r, g, b)))
    }
}

/// Sets the stroke color to an RGB (Red, Green, Blue) value. (PDF operator `RG`)
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
    pub const fn operator_name() -> &'static str {
        "RG"
    }

    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let r = operands.get_f32()?;
        let g = operands.get_f32()?;
        let b = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetRGBStroke(Self::new(r, g, b)))
    }
}

/// Sets the fill color to a CMYK (Cyan, Magenta, Yellow, Black/Key) value. (PDF operator `k`)
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
    pub const fn operator_name() -> &'static str {
        "k"
    }

    pub fn new(c: f32, m: f32, y: f32, k: f32) -> Self {
        Self { c, m, y, k }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let c = operands.get_f32()?;
        let m = operands.get_f32()?;
        let y = operands.get_f32()?;
        let k = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetCMYKFill(Self::new(c, m, y, k)))
    }
}

/// Sets the stroke color to a CMYK (Cyan, Magenta, Yellow, Black/Key) value. (PDF operator `K`)
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
    pub const fn operator_name() -> &'static str {
        "K"
    }

    pub fn new(c: f32, m: f32, y: f32, k: f32) -> Self {
        Self { c, m, y, k }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let c = operands.get_f32()?;
        let m = operands.get_f32()?;
        let y = operands.get_f32()?;
        let k = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetCMYKStroke(Self::new(c, m, y, k)))
    }
}
