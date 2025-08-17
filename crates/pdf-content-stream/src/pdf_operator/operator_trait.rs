use crate::pdf_operator_backend::PdfOperatorBackend;

/// Represents a PDF content stream operator.
///
/// This trait provides metadata about a PDF operator, such as its name
/// (the string representation used in PDF content streams) and the number
/// of operands it expects.
///
/// Implementors of this trait are typically structs that represent specific
/// PDF operators (e.g., `MoveTo`, `SetLineWidth`).
pub trait PdfOperator {
    /// The string representation of the PDF operator (e.g., "m", "BT", "rg").
    const NAME: &'static str;

    /// The number of operands this operator consumes from the operand stack.
    const OPERAND_COUNT: Option<usize>;

    /// Reads and consumes the necessary operands from the provided `Operands`
    /// slice and constructs the specific `PdfOperatorVariant`.
    fn read(
        operands: &mut crate::pdf_operator::Operands,
    ) -> Result<crate::pdf_operator::PdfOperatorVariant, crate::error::PdfOperatorError>;

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), T::ErrorType> {
        todo!("Unimplemented operator {}", Self::NAME)
    }
}
