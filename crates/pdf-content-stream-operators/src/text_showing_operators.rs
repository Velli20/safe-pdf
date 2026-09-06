use std::sync::Arc;

use crate::PdfTextItem;
use crate::operands::Operands;
use crate::operator_trait::PdfOperator;
use crate::{
    error::PdfOperatorError,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
};
use pdf_object_reader::{
    object_error::ObjectError, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
};

/// Shows a text string.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowText {
    /// The text item to be shown. Its bytes are typically encoded according to the font's
    /// encoding.
    text: PdfTextItem,
}

impl ShowText {
    pub fn new(text: PdfTextItem) -> Self {
        Self { text }
    }
}

impl PdfOperator for ShowText {
    const NAME: &'static [u8] = b"Tj";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let text = operands.get_string_bytes()?;
        Ok(PdfOperatorVariant::ShowText(Self::new(PdfTextItem::Text(
            text,
        ))))
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        backend.show_text(&self.text)
    }
}

/// Moves to the next line and shows a text string.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveNextLineShowText {
    /// The text string to be shown.
    text: PdfTextItem,
}

impl MoveNextLineShowText {
    pub fn new(text: PdfTextItem) -> Self {
        Self { text }
    }
}

impl PdfOperator for MoveNextLineShowText {
    const NAME: &'static [u8] = b"'";

    const OPERAND_COUNT: Option<usize> = Some(1);

    fn read(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        let text = operands.get_string_bytes()?;
        Ok(PdfOperatorVariant::MoveNextLineShowText(Self::new(
            PdfTextItem::Text(text),
        )))
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
    text: PdfTextItem,
}

impl SetSpacingMoveShowText {
    pub fn new(word_spacing: f32, char_spacing: f32, text: PdfTextItem) -> Self {
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
            PdfTextItem::Text(text),
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
    /// A vector of `PdfTextItem`s, where each item is either a string to show
    /// or a numeric adjustment for positioning.
    elements: Vec<PdfTextItem>,
}

impl ShowTextArray {
    pub fn new(elements: Vec<PdfTextItem>) -> Self {
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
        for value in values.iter() {
            match value {
                ObjectVariant::String(value) => {
                    elements.push(PdfTextItem::Text(Arc::clone(&value.bytes)));
                }
                other => {
                    let amount = other.try_number::<f32>(&PassthroughResolver)?;
                    elements.push(PdfTextItem::Adjustment(amount));
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
        let text: Arc<[u8]> = Arc::from(b"text without a copy".to_vec());

        let original_pointer = text.as_ptr();

        let mut operands = Operands::from(vec![ObjectVariant::String(
            pdf_object_reader::pdf_string::PdfString {
                bytes: text,
                kind: pdf_object_reader::string_kind::StringKind::Literal,
            },
        )]);

        let operator = ShowText::read(&mut operands).expect("text should parse");
        let PdfOperatorVariant::ShowText(operator) = operator else {
            panic!("expected ShowText variant");
        };

        let PdfTextItem::Text(text) = operator.text else {
            panic!("expected text item");
        };
        assert_eq!(&*text, b"text without a copy");
        assert_eq!(text.as_ptr(), original_pointer);
    }

    #[test]
    fn spacing_move_show_text_groups_numeric_prefix_before_text() {
        let mut operands = Operands::from(vec![
            ObjectVariant::Real(1.5),
            ObjectVariant::Integer(2),
            pdf_object_reader::pdf_string::PdfString::from(
                b"text",
                pdf_object_reader::string_kind::StringKind::Literal,
            ),
        ]);

        let operator = SetSpacingMoveShowText::read(&mut operands)
            .expect("mixed operands should parse successfully");
        let PdfOperatorVariant::SetSpacingMoveShowText(operator) = operator else {
            panic!("expected SetSpacingMoveShowText variant");
        };

        assert_eq!(operator.word_spacing, 1.5);
        assert_eq!(operator.char_spacing, 2.0);
        assert_eq!(
            operator.text,
            PdfTextItem::Text(std::sync::Arc::from(&b"text"[..]))
        );
        assert!(operands.is_empty());
    }
}
