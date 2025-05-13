use crate::TextElement;
use crate::{
    error::PdfPainterError,
    pdf_operator::{Operands, PdfOperatorVariant},
};

/// Shows a text string. (PDF operator `Tj`)
#[derive(Debug, Clone, PartialEq)]
pub struct ShowText {
    /// The text string to be shown. The string is typically encoded according to the font's encoding.
    text: String,
}

impl ShowText {
    pub const fn operator_name() -> &'static str {
        "Tj"
    }

    pub fn new(text: String) -> Self {
        Self { text }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let text = operands.get_str()?;
        Ok(PdfOperatorVariant::ShowText(Self::new(text)))
    }
}

/// Moves to the next line and shows a text string. (PDF operator `'`)
/// This is equivalent to `MoveToNextLine` followed by `ShowText { text }`.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveNextLineShowText {
    /// The text string to be shown.
    text: String,
}

impl MoveNextLineShowText {
    pub const fn operator_name() -> &'static str {
        "'"
    }

    pub fn new(text: String) -> Self {
        Self { text }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let text = operands.get_str()?;
        Ok(PdfOperatorVariant::MoveNextLineShowText(Self::new(text)))
    }
}

/// Sets the word and character spacing, moves to the next line, and shows a text string. (PDF operator `"`)
/// This is equivalent to `SetWordSpacing { spacing: word_spacing }`,
/// `SetCharacterSpacing { spacing: char_spacing }`, and `MoveNextLineShowText { text }`.
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
    pub const fn operator_name() -> &'static str {
        "\""
    }

    pub fn new(word_spacing: f32, char_spacing: f32, text: String) -> Self {
        Self {
            word_spacing,
            char_spacing,
            text,
        }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let word_spacing = operands.get_f32()?;
        let char_spacing = operands.get_f32()?;
        let text = operands.get_str()?;
        Ok(PdfOperatorVariant::SetSpacingMoveShowText(Self::new(
            word_spacing,
            char_spacing,
            text,
        )))
    }
}

/// Shows one or more text strings, allowing individual glyph positioning. (PDF operator `TJ`)
/// The array can contain strings and numbers. Numbers specify an additional horizontal or vertical
/// displacement (depending on the writing mode) to apply before showing the next string or glyph.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowTextArray {
    /// A vector of `TextElement`s, where each element is either a string to show
    /// or a numeric adjustment for positioning.
    elements: Vec<TextElement>, // Assuming TextElement is defined elsewhere (e.g., in lib.rs or a common module)
}

impl ShowTextArray {
    pub const fn operator_name() -> &'static str {
        "TJ"
    }

    pub fn new(elements: Vec<TextElement>) -> Self {
        Self { elements }
    }

    pub fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfPainterError> {
        let elements = operands.get_text_element_array()?;
        Ok(PdfOperatorVariant::ShowTextArray(Self::new(elements)))
    }
}
