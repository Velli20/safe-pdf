use crate::{
    error::PdfOperatorError,
    operands::Operands,
    operator_trait::PdfOperator,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    variants::PdfOperatorVariant,
};

/// Begins a compatibility section (BX).
///
/// According to the PDF specification, a compatibility section allows
/// consumers to ignore any operators they do not recognize until the
/// matching EX operator is encountered. For our purposes, we parse and
/// expose these operators so unknown operators within can be tolerated
/// by higher-level logic. Backends may choose to track nesting depth,
/// but by default these operators are no-ops.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BeginCompatibility;

impl PdfOperator for BeginCompatibility {
    const NAME: &'static [u8] = b"BX";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::BeginCompatibility(BeginCompatibility))
    }

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
        Ok(())
    }
}

/// Ends a compatibility section (EX).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EndCompatibility;

impl PdfOperator for EndCompatibility {
    const NAME: &'static [u8] = b"EX";

    const OPERAND_COUNT: Option<usize> = Some(0);

    fn read(_operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError> {
        Ok(PdfOperatorVariant::EndCompatibility(EndCompatibility))
    }

    fn call<T: PdfOperatorBackend>(&self, _backend: &mut T) -> Result<(), BackendError<T>> {
        Ok(())
    }
}
