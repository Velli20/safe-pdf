use crate::{
    error::PdfOperatorError,
    pdf_operator::PdfOperatorVariant,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
};

/// Represents a PDF content stream operator.
///
/// This trait provides metadata about a PDF operator, such as its name
/// (the byte slice representation used in PDF content streams) and the number
/// of operands it expects.
///
/// Implementors of this trait are typically structs that represent specific
/// PDF operators (e.g., `MoveTo`, `SetLineWidth`).
pub trait PdfOperator {
    /// The byte slice representation of the PDF operator (e.g., b"m", b"BT", b"rg").
    const NAME: &'static [u8];

    /// The number of operands this operator consumes from the operand stack.
    const OPERAND_COUNT: Option<usize>;

    /// Reads and consumes the necessary operands from the provided `Operands`
    /// slice and constructs the specific `PdfOperatorVariant`.
    fn read(
        operands: &mut crate::pdf_operator::Operands,
    ) -> Result<PdfOperatorVariant, PdfOperatorError>;

    /// Optional custom parsing hook.
    ///
    /// Operators that cannot be parsed solely from the pre-collected `Operands`
    /// (for example, operators that need to read additional bytes from the
    /// content stream or use a non-standard grammar) should implement this
    /// method so they can perform custom parsing using the low-level
    /// `pdf_parser::parser::PdfParser`.
    ///
    /// The parser is positioned immediately after the operator token. If the
    /// operator successfully parses itself it should return `Ok(Some(variant))`.
    /// Returning `Ok(None)` (the default) signals that no custom parsing took
    /// place and the parsing machinery should fall back to the normal
    /// operands-based `read` implementation.
    ///
    /// Errors should be returned using `PdfOperatorError`.
    fn parse<'a>(
        _parser: &mut pdf_parser::parser::PdfParser<'a>,
    ) -> Result<Option<PdfOperatorVariant>, PdfOperatorError> {
        Ok(None)
    }

    fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>>;
}
