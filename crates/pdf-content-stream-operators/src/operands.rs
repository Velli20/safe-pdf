use crate::error::PdfOperatorError;
use pdf_object::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};

pub struct Operands(pub Vec<ObjectVariant>);

impl Operands {
    /// Peeks at the next operand without consuming it.
    ///
    /// Unlike [`take_next`], this method does not advance the internal slice.
    ///
    /// # Returns
    ///
    /// `Some` with a reference to the next operand, or `None` if there are no more operands.
    pub fn peek_next(&self) -> Option<&ObjectVariant> {
        self.0.first()
    }

    /// Pops and returns the next operand, advancing the internal slice.
    pub(crate) fn take_next(&mut self) -> Result<ObjectVariant, PdfOperatorError> {
        if !self.0.is_empty() {
            Ok(self.0.remove(0))
        } else {
            Err(PdfOperatorError::OperandMissing {
                expected: "an operand",
            })
        }
    }

    pub fn get_f32(&mut self) -> Result<f32, PdfOperatorError> {
        let value = self.take_next()?.try_number::<f32>(&PassthroughResolver)?;
        Ok(value)
    }

    pub fn get_str(&mut self) -> Result<String, PdfOperatorError> {
        let object = self.take_next()?;
        match object.try_str(&PassthroughResolver) {
            Ok(value) => Ok(value.to_owned()),
            Err(_) => Err(PdfOperatorError::OperandTypeMismatch {
                expected: "a string operand (HexString, Name, or LiteralString)",
                found: object.name(),
            }),
        }
    }

    pub fn get_bytes(&mut self) -> Result<Vec<u8>, PdfOperatorError> {
        let object = self.take_next()?;
        match object {
            ObjectVariant::HexString(s)
            | ObjectVariant::Name(s)
            | ObjectVariant::LiteralString(s) => Ok(s),
            _ => Err(PdfOperatorError::OperandTypeMismatch {
                expected: "a byte string operand (HexString, Name, or LiteralString)",
                found: object.name(),
            }),
        }
    }

    pub fn get_u8(&mut self) -> Result<u8, PdfOperatorError> {
        let value = self.take_next()?.try_number::<u8>(&PassthroughResolver)?;
        Ok(value)
    }

    pub fn get_f32_array(&mut self) -> Result<Vec<f32>, PdfOperatorError> {
        let array = self.take_next()?;
        Ok(array.try_vec_of::<f32>(&PassthroughResolver)?)
    }
}
