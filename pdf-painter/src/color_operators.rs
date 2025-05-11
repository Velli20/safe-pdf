use crate::PdfOperator;

/// Sets the fill color to a grayscale value. (PDF operator `g`)
/// The gray level applies to subsequent fill operations.
#[derive(Debug, Clone, PartialEq)]
pub struct SetGrayFill {
    /// The gray level, a value between 0.0 (black) and 1.0 (white).
    gray: f32,
}

impl PdfOperator for SetGrayFill {
    fn operator() -> &'static str {
        "g"
    }
}

impl SetGrayFill {
    pub fn new(gray: f32) -> Self {
        Self { gray }
    }
}

/// Sets the stroke color to a grayscale value. (PDF operator `G`)
/// The gray level applies to subsequent stroke operations.
#[derive(Debug, Clone, PartialEq)]
pub struct SetGrayStroke {
    /// The gray level, a value between 0.0 (black) and 1.0 (white).
    gray: f32,
}

impl PdfOperator for SetGrayStroke {
    fn operator() -> &'static str {
        "G"
    }
}

impl SetGrayStroke {
    pub fn new(gray: f32) -> Self {
        Self { gray }
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

impl PdfOperator for SetRGBFill {
    fn operator() -> &'static str {
        "rg"
    }
}

impl SetRGBFill {
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
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

impl PdfOperator for SetRGBStroke {
    fn operator() -> &'static str {
        "RG"
    }
}

impl SetRGBStroke {
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
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

impl PdfOperator for SetCMYKFill {
    fn operator() -> &'static str {
        "k"
    }
}

impl SetCMYKFill {
    pub fn new(c: f32, m: f32, y: f32, k: f32) -> Self {
        Self { c, m, y, k }
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

impl PdfOperator for SetCMYKStroke {
    fn operator() -> &'static str {
        "K"
    }
}

impl SetCMYKStroke {
    pub fn new(c: f32, m: f32, y: f32, k: f32) -> Self {
        Self { c, m, y, k }
    }
}
