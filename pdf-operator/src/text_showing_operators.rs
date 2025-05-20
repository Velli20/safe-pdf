use crate::TextElement;
use crate::pdf_operator::PdfOperator;
use crate::{
    error::PdfOperatorError,
    pdf_operator::{Operands, PdfOperatorVariant},
    pdf_operator_backend::PdfOperatorBackend,
};

/// Shows a text string.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowText {
    /// The text string to be shown. The string is typically encoded according to the font's encoding.
    text: String,
}

impl ShowText {
    pub fn new(text: String) -> Self {
        Self { text }
    }
}

impl PdfOperator for ShowText {
    const NAME: &'static str = "Tj";

    const OPERAND_COUNT: usize = 1;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let text = operands.get_str()?;
        Ok(PdfOperatorVariant::ShowText(Self::new(text)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.show_text(self.text.as_bytes())
    }
}

/// Moves to the next line and shows a text string.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveNextLineShowText {
    /// The text string to be shown.
    text: String,
}

impl MoveNextLineShowText {
    pub fn new(text: String) -> Self {
        Self { text }
    }
}

impl PdfOperator for MoveNextLineShowText {
    const NAME: &'static str = "'";

    const OPERAND_COUNT: usize = 1;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let text = operands.get_str()?;
        Ok(PdfOperatorVariant::MoveNextLineShowText(Self::new(text)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.move_to_next_line_and_show_text(self.text.as_bytes())
    }
}

/// Sets the word and character spacing, moves to the next line, and shows a text string.
/// This is equivalent to `SetWordSpacing`, `SetCharacterSpacing`, and `MoveNextLineShowText`.
#[derive(Debug, Clone, PartialEq)]
pub struct SetSpacingMoveShowText {
    /// The new word spacing to set before showing the text.
    word_spacing: f32,
    /// The new character spacing to set before showing the text.
    char_spacing: f32,
    /// The text string to be shown.
    text: String,
}

impl SetSpacingMoveShowText {
    pub fn new(word_spacing: f32, char_spacing: f32, text: String) -> Self {
        Self {
            word_spacing,
            char_spacing,
            text,
        }
    }
}

impl PdfOperator for SetSpacingMoveShowText {
    const NAME: &'static str = "\"";

    const OPERAND_COUNT: usize = 3;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let word_spacing = operands.get_f32()?;
        let char_spacing = operands.get_f32()?;
        let text = operands.get_str()?;
        Ok(PdfOperatorVariant::SetSpacingMoveShowText(Self::new(
            word_spacing,
            char_spacing,
            text,
        )))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.set_spacing_and_show_text(self.word_spacing, self.char_spacing, self.text.as_bytes())
    }
}

/// Shows one or more text strings, allowing individual glyph positioning.
/// The array can contain strings and numbers. Numbers specify an additional horizontal or vertical
/// displacement (depending on the writing mode) to apply before showing the next string or glyph.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowTextArray {
    /// A vector of `TextElement`s, where each element is either a string to show
    /// or a numeric adjustment for positioning.
    elements: Vec<TextElement>,
}

impl ShowTextArray {
    pub fn new(elements: Vec<TextElement>) -> Self {
        Self { elements }
    }
}

impl PdfOperator for ShowTextArray {
    const NAME: &'static str = "TJ";

    const OPERAND_COUNT: usize = 1;

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let elements = operands.get_text_element_array()?;
        Ok(PdfOperatorVariant::ShowTextArray(Self::new(elements)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        backend.show_text_with_glyph_positioning(&self.elements)
    }
}
