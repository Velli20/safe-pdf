use crate::{
    error::PdfPainterError,
    pdf_operator::{Operands, PdfOperatorVariant},
};

/// Sets the character spacing, `Tc`, which is a number expressed in unscaled text space units. (PDF operator `Tc`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetCharacterSpacing {
    /// The character spacing. Added to the horizontal displacement otherwise produced by showing a glyph.
    spacing: f32,
}

impl SetCharacterSpacing {
    pub const fn operator_name() -> &'static str {
        "Tc"
    }

    pub fn new(spacing: f32) -> Self {
        Self { spacing }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let spacing = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetCharacterSpacing(Self::new(spacing)))
    }
}

/// Sets the word spacing, `Tw`, which is a number expressed in unscaled text space units. (PDF operator `Tw`)
/// Word spacing is used by the `Tj`, `'`, and `"` operators.
#[derive(Debug, Clone, PartialEq)]
pub struct SetWordSpacing {
    /// The word spacing. Added to the character spacing when the character is a space (char code 32).
    spacing: f32,
}

impl SetWordSpacing {
    pub const fn operator_name() -> &'static str {
        "Tw"
    }

    pub fn new(spacing: f32) -> Self {
        Self { spacing }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let spacing = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetWordSpacing(Self::new(spacing)))
    }
}

/// Sets the horizontal scaling, `Tz`, which adjusts the width of glyphs by stretching or compressing them horizontally. (PDF operator `Tz`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetHorizontalScaling {
    /// The horizontal scaling factor as a percentage (e.g., 100.0 for 100% - no scaling).
    scale: f32,
}

impl SetHorizontalScaling {
    pub const fn operator_name() -> &'static str {
        "Tz"
    }

    pub fn new(scale: f32) -> Self {
        Self { scale }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let scale = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetHorizontalScaling(Self::new(scale)))
    }
}

/// Sets the text leading, `TL`, which is the vertical distance between the baselines of adjacent lines of text. (PDF operator `TL`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetLeading {
    /// The text leading, in unscaled text space units.
    leading: f32,
}

impl SetLeading {
    pub const fn operator_name() -> &'static str {
        "TL"
    }

    pub fn new(leading: f32) -> Self {
        Self { leading }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let leading = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetLeading(Self::new(leading)))
    }
}

/// Sets the text font, `Tf`, to a font resource in the resource dictionary and the text font size, `Tfs`, in unscaled text space units. (PDF operator `Tf`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetFont {
    /// The name of the font resource.
    name: String,
    /// The font size.
    size: f32,
}

impl SetFont {
    pub const fn operator_name() -> &'static str {
        "Tf"
    }

    pub fn new(name: String, size: f32) -> Self {
        Self { name, size }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let name = operands.get_name()?;
        let size = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetFont(Self::new(name, size)))
    }
}

/// Sets the text rendering mode, `Tr`, which determines whether text is filled, stroked, used as a clipping path, or some combination. (PDF operator `Tr`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetRenderingMode {
    /// The rendering mode.
    /// 0: Fill text.
    /// 1: Stroke text.
    /// 2: Fill, then stroke text.
    /// 3: Neither fill nor stroke text (invisible).
    /// 4: Fill text and add to path for clipping.
    /// 5: Stroke text and add to path for clipping.
    /// 6: Fill, then stroke text and add to path for clipping.
    /// 7: Add text to path for clipping.
    mode: u8,
}

impl SetRenderingMode {
    pub const fn operator_name() -> &'static str {
        "Tr"
    }

    pub fn new(mode: u8) -> Self {
        Self { mode }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let mode = operands.get_u8()?;
        Ok(PdfOperatorVariant::SetRenderingMode(Self::new(mode)))
    }
}

/// Sets the text rise, `Ts`, which specifies the vertical distance to shift the baseline of text relative to the current baseline. (PDF operator `Ts`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetTextRise {
    /// The text rise, in unscaled text space units. A positive value moves the baseline up.
    rise: f32,
}

impl SetTextRise {
    pub const fn operator_name() -> &'static str {
        "Ts"
    }

    pub fn new(rise: f32) -> Self {
        Self { rise }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let rise = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetTextRise(Self::new(rise)))
    }
}
