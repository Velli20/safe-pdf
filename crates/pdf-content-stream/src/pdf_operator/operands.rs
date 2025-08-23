use std::rc::Rc;

use pdf_object::{dictionary::Dictionary, ObjectVariant};

use crate::{error::PdfOperatorError, TextElement};

pub struct Operands<'a> {
    pub values: &'a [ObjectVariant],
}

impl<'a> Operands<'a> {
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
    fn take_and_map<T>(
        &mut self,
        expected_type_for_missing: &'static str,
        f: impl FnOnce(&'a ObjectVariant) -> Result<T, PdfOperatorError>,
    ) -> Result<T, PdfOperatorError> {
        match self.take_next() {
            Some(value) => f(value),
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

    /// Converts an ObjectVariant inside an array context to f32 with detailed expected messages.
    fn as_f32_for_array(
        val_obj: &ObjectVariant,
        expected_non_number: &'static str,
    ) -> Result<f32, PdfOperatorError> {
        match val_obj {
            ObjectVariant::Integer(_) | ObjectVariant::Real(_) => val_obj
                .as_number::<f32>()
                .map_err(|_| PdfOperatorError::InvalidOperandType {
                    expected_type: "Number (f32 convertible) in array",
                    found_type: val_obj.name(),
                }),
            _ => Err(PdfOperatorError::InvalidOperandType {
                expected_type: expected_non_number,
                found_type: val_obj.name(),
            }),
        }
    }

    pub fn get_f32(&mut self) -> Result<f32, PdfOperatorError> {
        self.take_and_map("Number (f32)", |value| {
            value.as_number::<f32>().map_err(|err| {
                PdfOperatorError::OperandNumericConversionError {
                    expected_type: "Number (f32)",
                    source: err,
                }
            })
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

    pub fn get_str(&mut self) -> Result<String, PdfOperatorError> {
        self.take_and_map("String", |value| {
            value.as_str().map(|s| s.to_string()).ok_or_else(|| {
                PdfOperatorError::InvalidOperandType {
                    expected_type: "String",
                    found_type: value.name(),
                }
            })
        })
    }

    pub fn get_bytes(&mut self) -> Result<&[u8], PdfOperatorError> {
        self.take_and_map("Vec<u8>", |value| {
            value
                .as_bytes()
                .ok_or_else(|| PdfOperatorError::InvalidOperandType {
                    expected_type: "Vec<u8>",
                    found_type: value.name(),
                })
        })
    }

    pub fn get_name(&mut self) -> Result<String, PdfOperatorError> {
        self.take_and_map("Name", |value| match value {
            ObjectVariant::Name(name) => Ok(name.clone()),
            _ => Err(PdfOperatorError::InvalidOperandType {
                expected_type: "Name",
                found_type: value.name(),
            }),
        })
    }

    pub fn get_u8(&mut self) -> Result<u8, PdfOperatorError> {
        self.take_and_map("Number (u8)", |value| {
            value
                .as_number::<u8>()
                .map_err(|err| PdfOperatorError::OperandNumericConversionError {
                    expected_type: "Number (u8)",
                    source: err,
                })
        })
    }

    pub fn get_text_element_array(&mut self) -> Result<Vec<TextElement>, PdfOperatorError> {
        let array_values = self.take_array("Array for TextElement")?;
        let mut elements = Vec::with_capacity(array_values.len());
        for val_obj in array_values {
            match val_obj {
                ObjectVariant::LiteralString(s) => {
                    elements.push(TextElement::Text { value: s.clone() })
                }
                _ => {
                    let amount = Self::as_f32_for_array(val_obj, "LiteralString or Number in array")?;
                    elements.push(TextElement::Adjustment { amount });
                }
            }
        }
        Ok(elements)
    }

    pub fn get_f32_array(&mut self) -> Result<Vec<f32>, PdfOperatorError> {
        let array_values = self.take_array("Array for f32")?;
        let mut numbers = Vec::with_capacity(array_values.len());
        for val_obj in array_values {
            numbers.push(Self::as_f32_for_array(val_obj, "Number in array")?);
        }
        Ok(numbers)
    }
}
