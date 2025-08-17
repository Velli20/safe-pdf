use std::rc::Rc;

use pdf_object::{ObjectVariant, dictionary::Dictionary};

use crate::{TextElement, error::PdfOperatorError};

pub struct Operands<'a> {
    pub values: &'a [ObjectVariant],
}

impl Operands<'_> {
    pub fn get_f32(&mut self) -> Result<f32, PdfOperatorError> {
        if let Some((value, rest)) = self.values.split_first() {
            self.values = rest;
            value.as_number::<f32>().map_err(|err| {
                PdfOperatorError::OperandNumericConversionError {
                    expected_type: "Number (f32)",
                    source: err,
                }
            })
        } else {
            Err(PdfOperatorError::MissingOperand {
                expected_type: "Number (f32)",
            })
        }
    }

    pub fn get_dictionary(&mut self) -> Result<Rc<Dictionary>, PdfOperatorError> {
        if let Some((value, rest)) = self.values.split_first() {
            self.values = rest;
            match value {
                ObjectVariant::Dictionary(dict) => Ok(std::rc::Rc::clone(dict)),
                _ => Err(PdfOperatorError::InvalidOperandType {
                    expected_type: "Dictionary",
                    found_type: value.name(),
                }),
            }
        } else {
            Err(PdfOperatorError::MissingOperand {
                expected_type: "Dictionary",
            })
        }
    }

    pub fn get_str(&mut self) -> Result<String, PdfOperatorError> {
        if let Some((value, rest)) = self.values.split_first() {
            self.values = rest;
            value.as_str().map(|s| s.to_string()).ok_or_else(|| {
                PdfOperatorError::InvalidOperandType {
                    expected_type: "String",
                    found_type: value.name(),
                }
            })
        } else {
            Err(PdfOperatorError::MissingOperand {
                expected_type: "String",
            })
        }
    }

    pub fn get_bytes(&mut self) -> Result<&[u8], PdfOperatorError> {
        if let Some((value, rest)) = self.values.split_first() {
            self.values = rest;
            value
                .as_bytes()
                .ok_or_else(|| PdfOperatorError::InvalidOperandType {
                    expected_type: "Vec<u8>",
                    found_type: value.name(),
                })
        } else {
            Err(PdfOperatorError::MissingOperand {
                expected_type: "Vec<u8>",
            })
        }
    }

    pub fn get_name(&mut self) -> Result<String, PdfOperatorError> {
        if let Some((value, rest)) = self.values.split_first() {
            self.values = rest;
            match value {
                ObjectVariant::Name(name) => Ok(name.clone()),
                _ => Err(PdfOperatorError::InvalidOperandType {
                    expected_type: "Name",
                    found_type: value.name(),
                }),
            }
        } else {
            Err(PdfOperatorError::MissingOperand {
                expected_type: "Name",
            })
        }
    }

    pub fn get_u8(&mut self) -> Result<u8, PdfOperatorError> {
        if let Some((value, rest)) = self.values.split_first() {
            self.values = rest;
            value
                .as_number::<u8>()
                .map_err(|err| PdfOperatorError::OperandNumericConversionError {
                    expected_type: "Number (u8)",
                    source: err,
                })
        } else {
            Err(PdfOperatorError::MissingOperand {
                expected_type: "Number (u8)",
            })
        }
    }

    pub fn get_text_element_array(&mut self) -> Result<Vec<TextElement>, PdfOperatorError> {
        if let Some((first_operand, rest_operands)) = self.values.split_first() {
            self.values = rest_operands;
            if let ObjectVariant::Array(array_values) = first_operand {
                let mut elements = Vec::with_capacity(array_values.len());
                for val_obj in array_values {
                    match val_obj {
                        ObjectVariant::LiteralString(s) => {
                            elements.push(TextElement::Text { value: s.clone() })
                        }
                        ObjectVariant::Integer(_) | ObjectVariant::Real(_) => {
                            if let Ok(amount) = val_obj.as_number::<f32>() {
                                elements.push(TextElement::Adjustment { amount });
                            } else {
                                return Err(PdfOperatorError::InvalidOperandType {
                                    expected_type: "Number (f32 convertible) in array",
                                    found_type: val_obj.name(),
                                });
                            }
                        }
                        _ => {
                            return Err(PdfOperatorError::InvalidOperandType {
                                expected_type: "LiteralString or Number in array",
                                found_type: val_obj.name(),
                            });
                        }
                    }
                }
                Ok(elements)
            } else {
                Err(PdfOperatorError::InvalidOperandType {
                    expected_type: "Array",
                    found_type: first_operand.name(),
                })
            }
        } else {
            Err(PdfOperatorError::MissingOperand {
                expected_type: "Array for TextElement",
            })
        }
    }

    pub fn get_f32_array(&mut self) -> Result<Vec<f32>, PdfOperatorError> {
        if let Some((first_operand, rest_operands)) = self.values.split_first() {
            self.values = rest_operands;
            if let ObjectVariant::Array(array_values) = first_operand {
                let mut numbers = Vec::with_capacity(array_values.len());
                for val_obj in array_values {
                    match val_obj {
                        ObjectVariant::Integer(_) | ObjectVariant::Real(_) => {
                            if let Ok(num_f32) = val_obj.as_number::<f32>() {
                                numbers.push(num_f32);
                            } else {
                                return Err(PdfOperatorError::InvalidOperandType {
                                    expected_type: "Number (f32 convertible) in array",
                                    found_type: val_obj.name(),
                                });
                            }
                        }
                        _ => {
                            return Err(PdfOperatorError::InvalidOperandType {
                                expected_type: "Number in array",
                                found_type: val_obj.name(),
                            });
                        }
                    }
                }
                Ok(numbers)
            } else {
                Err(PdfOperatorError::InvalidOperandType {
                    expected_type: "Array",
                    found_type: first_operand.name(),
                })
            }
        } else {
            Err(PdfOperatorError::MissingOperand {
                expected_type: "Array for f32",
            })
        }
    }
}
