use crate::OperandContext;
use crate::PdfTextItem;
use crate::pdf_operator_backend::{BackendError, PdfOperatorBackend};
use pdf_object_reader::{ReadResult, pdf_string::PdfString};

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

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let text = operands.at::<PdfString>(0)?.bytes;
        Ok(Self::new(PdfTextItem::Text(text)))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
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

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let text = operands.at::<PdfString>(0)?.bytes;
        Ok(Self::new(PdfTextItem::Text(text)))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
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

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        let [word_spacing, char_spacing] = operands.read_array()?;
        let text = operands.at::<PdfString>(2)?.bytes;

        Ok(Self::new(
            word_spacing,
            char_spacing,
            PdfTextItem::Text(text),
        ))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
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

    /// Reads the operator from its operands.
    pub(crate) fn read(operands: &mut OperandContext<'_, '_>) -> ReadResult<Self> {
        Ok(Self::new(operands.at(0)?))
    }

    /// Dispatches the operator to `backend`.
    pub(crate) fn call<T: PdfOperatorBackend>(
        &self,
        backend: &mut T,
    ) -> Result<(), BackendError<T>> {
        backend.show_text_with_glyph_positioning(&self.elements)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use pdf_object_reader::{
        ArrayContext, ObjectReader, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant,
    };

    use super::*;

    #[test]
    fn show_text_moves_the_operand_byte_allocation() {
        let text: Arc<[u8]> = Arc::from(b"text without a copy".to_vec());

        let original_pointer = text.as_ptr();

        let operands = [ObjectVariant::String(
            pdf_object_reader::pdf_string::PdfString {
                bytes: text,
                kind: pdf_object_reader::string_kind::StringKind::Literal,
            },
        )];
        let reader = ObjectReader::new(PassthroughResolver);
        let mut session = reader.session();

        let operator = ShowText::read(&mut ArrayContext::new(&operands, &mut session))
            .expect("text should parse");
        let PdfTextItem::Text(text) = operator.text else {
            panic!("expected text item");
        };
        assert_eq!(&*text, b"text without a copy");
        assert_eq!(text.as_ptr(), original_pointer);
    }

    #[test]
    fn spacing_move_show_text_groups_numeric_prefix_before_text() {
        let operands = [
            ObjectVariant::Real(1.5),
            ObjectVariant::Integer(2),
            pdf_object_reader::pdf_string::PdfString::from(
                b"text",
                pdf_object_reader::string_kind::StringKind::Literal,
            ),
        ];
        let reader = ObjectReader::new(PassthroughResolver);
        let mut session = reader.session();

        let operator =
            SetSpacingMoveShowText::read(&mut ArrayContext::new(&operands, &mut session))
                .expect("mixed operands should parse successfully");
        assert_eq!(operator.word_spacing, 1.5);
        assert_eq!(operator.char_spacing, 2.0);
        assert_eq!(operator.text, PdfTextItem::Text(Arc::from(&b"text"[..])));
    }
}
