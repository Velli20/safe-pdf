use std::{borrow::Cow, rc::Rc};

use pdf_object::{ObjectVariant, dictionary::Dictionary, object_resolver::UnimplementedResolver};

use crate::{TextElement, error::PdfOperatorError};

pub struct Operands<'a> {
    pub values: &'a [ObjectVariant],
}

impl<'a> Operands<'a> {
    /// Peeks at the next operand without consuming it.
    ///
    /// Unlike [`take_next`], this method does not advance the internal slice.
    ///
    /// # Returns
    ///
    /// `Some` with a reference to the next operand, or `None` if there are no more operands.
    pub fn peek_next(&self) -> Option<&'a ObjectVariant> {
        self.values.first()
    }

    /// Pops and returns the next operand, advancing the internal slice.
    fn take_next(&mut self) -> Option<&'a ObjectVariant> {
        if let Some((value, rest)) = self.values.split_first() {
            self.values = rest;
            Some(value)
        } else {
            None
        }
    }

    /// Generic helper to pop an operand and convert it with a closure, mapping a missing operand
    /// into a consistent error that mentions the expected type.
    fn take_and_map<T, E: Into<PdfOperatorError>>(
        &mut self,
        expected_type_for_missing: &'static str,
        f: impl FnOnce(&'a ObjectVariant) -> Result<T, E>,
    ) -> Result<T, PdfOperatorError> {
        match self.take_next() {
            Some(value) => f(value).map_err(Into::into),
            None => Err(PdfOperatorError::MissingOperand {
                expected_type: expected_type_for_missing,
            }),
        }
    }

    /// Pops the next operand and returns it as an Array slice, or an error.
    fn take_array(
        &mut self,
        expected_type_for_missing: &'static str,
    ) -> Result<&'a [ObjectVariant], PdfOperatorError> {
        self.take_and_map(expected_type_for_missing, |value| match value {
            ObjectVariant::Array(arr) => Ok(arr.as_slice()),
            _ => Err(PdfOperatorError::InvalidOperandType {
                expected_type: "Array",
                found_type: value.name(),
            }),
        })
    }

    pub fn get_f32(&mut self) -> Result<f32, PdfOperatorError> {
        self.take_and_map("Number (f32)", |value| {
            value.try_number::<f32>(&UnimplementedResolver)
        })
    }

    pub fn get_dictionary(&mut self) -> Result<Rc<Dictionary>, PdfOperatorError> {
        self.take_and_map("Dictionary", |value| match value {
            ObjectVariant::Dictionary(dict) => Ok(std::rc::Rc::clone(dict)),
            _ => Err(PdfOperatorError::InvalidOperandType {
                expected_type: "Dictionary",
                found_type: value.name(),
            }),
        })
    }

    pub fn get_str(&'_ mut self) -> Result<Cow<'_, str>, PdfOperatorError> {
        self.take_and_map("String", |value| value.try_str(&UnimplementedResolver))
    }

    pub fn get_bytes(&mut self) -> Result<&[u8], PdfOperatorError> {
        self.take_and_map("Vec<u8>", |value| value.try_bytes(&UnimplementedResolver))
    }

    pub fn get_u8(&mut self) -> Result<u8, PdfOperatorError> {
        self.take_and_map("Number (u8)", |value| {
            value.try_number::<u8>(&UnimplementedResolver)
        })
    }

    pub fn get_text_element_array(&mut self) -> Result<Vec<TextElement>, PdfOperatorError> {
        let array_values = self.take_array("Array for TextElement")?;
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
                    let amount = val_obj.try_number::<f32>(&UnimplementedResolver)?;
                    elements.push(TextElement::Adjustment { amount });
                }
            }
        }
        Ok(elements)
    }

    pub fn get_f32_array(&mut self) -> Result<Vec<f32>, PdfOperatorError> {
        let array = self.take_next().ok_or(PdfOperatorError::MissingOperand {
            expected_type: "Array for f32",
        })?;
        Ok(array.try_vec_of::<f32>(&UnimplementedResolver)?)
    }
}
