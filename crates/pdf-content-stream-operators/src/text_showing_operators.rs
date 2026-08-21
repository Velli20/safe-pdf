use crate::TextElement;
use crate::operands::Operands;
use crate::operator_trait::PdfOperator;
use crate::{
    error::PdfOperatorError,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
};
use pdf_object::{
    error::ObjectError, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
};

/// Shows a text string.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowText {
    /// An array of bytes of the text string to be shown. The string is typically encoded
    /// according to the font's encoding.
    text: Vec<u8>,
}

impl ShowText {
    pub fn new(text: Vec<u8>) -> Self {
        Self { text }
    }
}

impl PdfOperator for ShowText {
    const NAME: &'static [u8] = b"Tj";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let text = operands.get_string_bytes()?;
        Ok(PdfOperatorVariant::ShowText(Self::new(text)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.show_text(&self.text)
    }
}

/// Moves to the next line and shows a text string.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveNextLineShowText {
    /// The text string to be shown.
    text: Vec<u8>,
}

impl MoveNextLineShowText {
    pub fn new(text: Vec<u8>) -> Self {
        Self { text }
    }
}

impl PdfOperator for MoveNextLineShowText {
    const NAME: &'static [u8] = b"'";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let text = operands.get_string_bytes()?;
        Ok(PdfOperatorVariant::MoveNextLineShowText(Self::new(text)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.move_to_next_line_and_show_text(&self.text)
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
    text: Vec<u8>,
}

impl SetSpacingMoveShowText {
    pub fn new(word_spacing: f32, char_spacing: f32, text: Vec<u8>) -> Self {
        Self {
            word_spacing,
            char_spacing,
            text,
        }
    }
}

impl PdfOperator for SetSpacingMoveShowText {
    const NAME: &'static [u8] = b"\"";

    const OPERAND_COUNT: Option<usize> = Some(3);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let [word_spacing, char_spacing] = operands.try_array_of::<f32, 2>()?;
        let text = operands.get_string_bytes()?;

        Ok(PdfOperatorVariant::SetSpacingMoveShowText(Self::new(
            word_spacing,
            char_spacing,
            text,
        )))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.set_spacing_and_show_text(self.word_spacing, self.char_spacing, &self.text)
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
    const NAME: &'static [u8] = b"TJ";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let object = operands.take_next()?;
        let ObjectVariant::Array(values) = object else {
            return Err(ObjectError::TypeMismatch("Array", object.name()).into());
        };

        let mut elements = Vec::with_capacity(values.len());
        for value in values {
            match value {
                ObjectVariant::HexString(value) => {
                    elements.push(TextElement::HexString { value });
                }
                ObjectVariant::LiteralString(value) => {
                    elements.push(TextElement::Text { value });
                }
                other => {
                    let amount = other.try_number::<f32>(&PassthroughResolver)?;
                    elements.push(TextElement::Adjustment { amount });
                }
            }
        }

        Ok(PdfOperatorVariant::ShowTextArray(Self::new(elements)))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.show_text_with_glyph_positioning(&self.elements)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn show_text_moves_the_operand_byte_allocation() {
        let text = b"text without a copy".to_vec();
        let original_pointer = text.as_ptr();
        let mut operands = Operands::from(vec![ObjectVariant::LiteralString(text)]);

        let operator = ShowText::read(&mut operands).expect("text should parse");
        let PdfOperatorVariant::ShowText(operator) = operator else {
            panic!("expected ShowText variant");
        };

        assert_eq!(operator.text, b"text without a copy");
        assert_eq!(operator.text.as_ptr(), original_pointer);
    }

    #[test]
    fn spacing_move_show_text_groups_numeric_prefix_before_text() {
        let mut operands = Operands::from(vec![
            ObjectVariant::Real(1.5),
            ObjectVariant::Integer(2),
            ObjectVariant::LiteralString(b"text".to_vec()),
        ]);

        let operator = SetSpacingMoveShowText::read(&mut operands)
            .expect("mixed operands should parse successfully");
        let PdfOperatorVariant::SetSpacingMoveShowText(operator) = operator else {
            panic!("expected SetSpacingMoveShowText variant");
        };

        assert_eq!(operator.word_spacing, 1.5);
        assert_eq!(operator.char_spacing, 2.0);
        assert_eq!(operator.text, b"text");
        assert!(operands.is_empty());
    }
}
