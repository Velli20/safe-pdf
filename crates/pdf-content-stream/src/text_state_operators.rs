use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
    pdf_operator_backend::PdfOperatorBackend,
};
use num_traits::FromPrimitive;
use pdf_graphics::TextRenderingMode;

/// Sets the character spacing, `Tc`, which is a number expressed in unscaled text space units.
#[derive(Debug, Clone, PartialEq)]
pub struct SetCharacterSpacing {
    /// The character spacing. Added to the horizontal displacement otherwise produced by showing a glyph.
    spacing: f32,
}

impl SetCharacterSpacing {
    pub fn new(spacing: f32) -> Self {
        Self { spacing }
    }
}

impl PdfOperator for SetCharacterSpacing {
    const NAME: &'static str = "Tc";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let spacing = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetCharacterSpacing(Self::new(spacing)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.set_character_spacing(self.spacing)
    }
}

/// Sets the word spacing, `Tw`, which is a number expressed in unscaled text space units.
/// Word spacing is used by the `Tj`, `'`, and `"` operators.
#[derive(Debug, Clone, PartialEq)]
pub struct SetWordSpacing {
    /// The word spacing. Added to the character spacing when the character is a space (char code 32).
    spacing: f32,
}

impl SetWordSpacing {
    pub fn new(spacing: f32) -> Self {
        Self { spacing }
    }
}

impl PdfOperator for SetWordSpacing {
    const NAME: &'static str = "Tw";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let spacing = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetWordSpacing(Self::new(spacing)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.set_word_spacing(self.spacing)
    }
}

/// Sets the horizontal scaling, `Tz`, which adjusts the width of glyphs by
/// stretching or compressing them horizontally.
#[derive(Debug, Clone, PartialEq)]
pub struct SetHorizontalScaling {
    /// The horizontal scaling factor as a percentage (e.g., 100.0 for 100% - no scaling).
    scale: f32,
}

impl SetHorizontalScaling {
    pub fn new(scale: f32) -> Self {
        Self { scale }
    }
}

impl PdfOperator for SetHorizontalScaling {
    const NAME: &'static str = "Tz";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let scale = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetHorizontalScaling(Self::new(scale)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.set_horizontal_text_scaling(self.scale)
    }
}

/// Sets the text leading, `TL`, which is the vertical distance between the baselines of
/// adjacent lines of text.
#[derive(Debug, Clone, PartialEq)]
pub struct SetLeading {
    /// The text leading, in unscaled text space units.
    leading: f32,
}

impl SetLeading {
    pub fn new(leading: f32) -> Self {
        Self { leading }
    }
}

impl PdfOperator for SetLeading {
    const NAME: &'static str = "TL";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let leading = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetLeading(Self::new(leading)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.set_text_leading(self.leading)
    }
}

/// Sets the text font, `Tf`, to a font resource in the resource dictionary and the text
/// font size, `Tfs`, in unscaled text space units.
#[derive(Debug, Clone, PartialEq)]
pub struct SetFont {
    /// The name of the font resource.
    name: String,
    /// The font size.
    size: f32,
}

impl SetFont {
    pub fn new(name: String, size: f32) -> Self {
        Self { name, size }
    }
}

impl PdfOperator for SetFont {
    const NAME: &'static str = "Tf";

    const OPERAND_COUNT: Option<usize> = Some(2);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let name = operands.get_name()?;
        let size = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetFont(Self::new(name, size)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.set_font_and_size(&self.name, self.size)
    }
}

/// Sets the text rendering mode, which determines whether text is filled, stroked,
/// used as a clipping path, or some combination.
#[derive(Debug, Clone, PartialEq)]
pub struct SetRenderingMode {
    /// The rendering mode.
    mode: TextRenderingMode,
}

impl SetRenderingMode {
    pub fn new(mode: u8) -> Result<Self, PdfOperatorError> {
        match TextRenderingMode::from_u8(mode) {
            Some(mode) => Ok(Self { mode }),
            None => Err(PdfOperatorError::InvalidOperandValue {
                expected: "One of the valid text rendering modes (0-7)",
                value: mode.to_string(),
            }),
        }
    }
}

impl PdfOperator for SetRenderingMode {
    const NAME: &'static str = "Tr";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let mode = operands.get_u8()?;
        Ok(PdfOperatorVariant::SetRenderingMode(Self::new(mode)?))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.set_text_rendering_mode(self.mode)
    }
}

/// Sets the text rise, `Ts`, which specifies the vertical distance
/// to shift the baseline of text relative to the current baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct SetTextRise {
    /// The text rise, in unscaled text space units. A positive value moves the baseline up.
    rise: f32,
}

impl SetTextRise {
    pub fn new(rise: f32) -> Self {
        Self { rise }
    }
}

impl PdfOperator for SetTextRise {
    const NAME: &'static str = "Ts";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let rise = operands.get_f32()?;
        Ok(PdfOperatorVariant::SetTextRise(Self::new(rise)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.set_text_rise(self.rise)
    }
}
