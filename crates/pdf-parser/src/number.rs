use pdf_object::object_variant::ObjectVariant;
use pdf_tokenizer::PdfToken;
use thiserror::Error;

use crate::{error::ParserError, parser::PdfParser};

#[derive(Debug, PartialEq, Error)]
pub enum NumberError {
    #[error("Failed to parse integral part of number: {err}")]
    IntegralPartError { err: String },
    #[error("Failed to parse fractional part of number: {err}")]
    FractionalPartError { err: String },
    #[error("Failed to parse '{number_str}' as a real number: {source}")]
    RealNumberParseError {
        number_str: String,
        #[source]
        source: std::num::ParseFloatError,
    },
    #[error("Numeric value overflow")]
    NumericValueOverflow,
}

impl PdfParser<'_> {
    /// Parses a PDF numeric object (integer or real) from the current position in the input stream.
    ///
    /// # Returns
    ///
    /// A `Number` object containing the parsed integer (`i64`) or real (`f64`) value,
    /// or a `ParserError` on failure.
    pub fn parse_number(&mut self) -> Result<ObjectVariant, ParserError> {
        let mut has_minus = false;

        // 1. Check for optional sign.
        if let Some(PdfToken::Plus) = self.tokenizer.peek() {
            self.tokenizer.read();
        } else if let Some(PdfToken::Minus) = self.tokenizer.peek() {
            self.tokenizer.read();
            has_minus = true;
        }

        // 2. Parse leading digits (integral part).
        let mut digits = if let Some(PdfToken::Period) = self.tokenizer.peek() {
            0
        } else {
            self.read_number::<i64>(false)?
        };

        // 3. Check for decimal point
        if let Some(PdfToken::Period) = self.tokenizer.peek() {
            self.tokenizer.read();
            // 4. Parse fractional part as a string to preserve leading zeros.
            let fraction_bytes = self.tokenizer.read_while_u8(|b| b.is_ascii_digit());
            let fraction_str = String::from_utf8_lossy(fraction_bytes);

            // A number can be represented as `.d` but not as `.`
            if digits == 0 && fraction_str.is_empty() {
                return Err(NumberError::FractionalPartError {
                    err: "Invalid real number: missing digits after decimal point.".to_string(),
                }
                .into());
            }

            // 5. Combine integral and fractional parts.
            let number_str = if has_minus {
                format!("-{}.{}", digits, fraction_str)
            } else {
                format!("{}.{}", digits, fraction_str)
            };

            // 6. Convert to f64.
            let number = number_str
                .parse::<f64>()
                .map_err(|source| NumberError::RealNumberParseError { number_str, source })?;

            if let Some(d) = self.tokenizer.data().first().copied()
                && !Self::is_pdf_delimiter(d)
            {
                return Err(NumberError::FractionalPartError {
                    err: format!("Missing delimiter after number, found '{}'", char::from(d)),
                }
                .into());
            }
            self.skip_whitespace();
            Ok(ObjectVariant::Real(number))
        } else {
            // 7. No decimal point, parse as integer.
            self.skip_whitespace();
            if has_minus {
                digits = digits
                    .checked_neg()
                    .ok_or(NumberError::NumericValueOverflow)?;
            }
            Ok(ObjectVariant::Integer(digits))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_number_valid_integers() {
        let valid_inputs: Vec<(&[u8], i64)> = vec![
            (b"123 ", 123),
            (b"-456 ", -456),
            (b"+789 ", 789),
            (b"0 ", 0),
            (b"2147483647 ", 2147483647),
            (b"-2147483647 ", -2147483647),
        ];

        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_number().unwrap();
            assert_eq!(result, ObjectVariant::Integer(expected));
        }
    }

    #[test]
    fn test_parse_number_valid_floats() {
        let valid_inputs: Vec<(&[u8], f64)> = vec![
            (b"123.456 ", 123.456),
            (b"-0.789 ", -0.789),
            #[allow(clippy::approx_constant)]
            (b"+3.14 ", 3.14),
            (b"0.0 ", 0.0),
            (b".00048828125", 0.00048828125),
            (b"-.00048828125", -0.00048828125),
        ];

        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_number().unwrap();
            assert_eq!(result, ObjectVariant::Real(expected));
        }
    }

    #[test]
    fn test_parse_number_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![
            b"--42",    // double minus
            b"++17",    // double plus
            b"+-5",     // invalid combination
            b"4,200",   // comma not allowed
            b"123abc ", // Mixed numeric and non-numeric characters
            b".",
            b"-.",
            //      b"--123 ",        // Invalid double minus
            //     b"123..456 ",     // Invalid double decimal point
            //    b"123.456.789 ",  // Multiple decimal points
        ];

        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_number();
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
