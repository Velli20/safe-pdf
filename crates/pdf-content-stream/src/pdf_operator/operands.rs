use crate::{TextElement, error::PdfOperatorError};
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
    fn take_next(&mut self) -> Result<ObjectVariant, PdfOperatorError> {
        if !self.0.is_empty() {
            Ok(self.0.remove(0))
        } else {
            Err(PdfOperatorError::MissingOperand {
                expected_type: "Operand",
            })
        }
    }

    pub fn get_f32(&mut self) -> Result<f32, PdfOperatorError> {
        let value = self.take_next()?.try_number::<f32>(&PassthroughResolver)?;
        Ok(value)
    }

    pub fn get_str(&mut self) -> Result<String, PdfOperatorError> {
        match self.take_next()? {
            ObjectVariant::HexString(s)
            | ObjectVariant::Name(s)
            | ObjectVariant::LiteralString(s) => Ok(String::from_utf8_lossy(&s).into_owned()),
            other => Err(PdfOperatorError::InvalidOperandType {
                expected_type: "String (HexString, Name, or LiteralString)",
                found_type: other.name(),
            }),
        }
    }

    pub fn get_bytes(&mut self) -> Result<Vec<u8>, PdfOperatorError> {
        let object = self.take_next()?;
        match object {
            ObjectVariant::HexString(s)
            | ObjectVariant::Name(s)
            | ObjectVariant::LiteralString(s) => Ok(s),
            _ => Err(PdfOperatorError::InvalidOperandType {
                expected_type: "Bytes (HexString, Name, or LiteralString)",
                found_type: object.name(),
            }),
        }
    }

    pub fn get_u8(&mut self) -> Result<u8, PdfOperatorError> {
        let value = self.take_next()?.try_number::<u8>(&PassthroughResolver)?;
        Ok(value)
    }

    pub fn get_text_element_array(&mut self) -> Result<Vec<TextElement>, PdfOperatorError> {
        let object = self.take_next()?;
        let array_values = object.try_array(&PassthroughResolver)?;

        let mut elements = Vec::with_capacity(array_values.len());
        for val_obj in array_values {
            match val_obj {
                ObjectVariant::HexString(s) => {
                    elements.push(TextElement::HexString { value: s.clone() })
                }
                ObjectVariant::LiteralString(s) => {
                    elements.push(TextElement::Text { value: s.clone() })
                }
                _ => {
                    let amount = val_obj.try_number::<f32>(&PassthroughResolver)?;
                    elements.push(TextElement::Adjustment { amount });
                }
            }
        }
        Ok(elements)
    }

    pub fn get_f32_array(&mut self) -> Result<Vec<f32>, PdfOperatorError> {
        let array = self.take_next()?;
        Ok(array.try_vec_of::<f32>(&PassthroughResolver)?)
    }
}
