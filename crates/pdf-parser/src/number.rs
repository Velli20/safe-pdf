use pdf_object::object_variant::ObjectVariant;
use pdf_tokenizer::PdfToken;
use thiserror::Error;

use crate::{error::ParserError, parser::PdfParser};

#[derive(Debug, PartialEq, Error)]
pub enum NumberError {
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
    #[error("Missing delimiter after number, found '{0}'")]
    MissingDelimiterAfterNumber(char),
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

        // 2. Parse leading digits (integral part). Track whether input started with '.'
        //    so we can distinguish ".5" (no leading digits, integer_part is a sentinel 0)
        //    from "0.5" (leading zero, integer_part is a parsed 0). The two cases have
        //    different validity rules: bare "." is invalid but "0." is valid.
        let started_with_period = matches!(self.tokenizer.peek(), Some(PdfToken::Period));
        let integer_part: i64 = if started_with_period {
            0
        } else {
            self.read_number::<i64>(false)?
        };

        // 3. Check for decimal point.
        if let Some(PdfToken::Period) = self.tokenizer.peek() {
            self.tokenizer.read();

            // 4. Parse fractional digits. Preserve them as raw bytes to avoid a
            //    lossy-conversion → format! → re-parse round-trip.
            let fraction_bytes = self.tokenizer.read_while_u8(|b| b.is_ascii_digit());

            // fraction_bytes contains only ASCII digits (guaranteed by the predicate above),
            // so from_utf8 is always successful here.
            let fraction_str = std::str::from_utf8(fraction_bytes).map_err(|_| {
                NumberError::FractionalPartError {
                    err: "digit sequence contains non-UTF8 bytes".to_string(),
                }
            })?;

            // "." and "-." are invalid; "0." and "0.0" are valid real numbers.
            if started_with_period && fraction_str.is_empty() {
                return Err(NumberError::FractionalPartError {
                    err: "Invalid real number: missing digits after decimal point.".to_string(),
                }
                .into());
            }

            // 5. Compute the real value directly without building a formatted string.
            //    Parse fractional digits as their integer value (e.g. "456" → 456.0),
            //    then scale by 10^-len to get the fractional contribution.
            let frac_value: f64 = if fraction_str.is_empty() {
                0.0
            } else {
                let exp = i32::try_from(fraction_str.len())
                    .map_err(|_| NumberError::NumericValueOverflow)?;
                let frac_digits = fraction_str.parse::<f64>().map_err(|source| {
                    NumberError::RealNumberParseError {
                        number_str: fraction_str.to_string(),
                        source,
                    }
                })?;
                frac_digits / 10_f64.powi(exp)
            };

            // i64 → f64 is a well-defined lossy cast; no From impl exists in std for this pair.
            #[allow(clippy::as_conversions)]
            let value = integer_part as f64 + frac_value;
            let number = if has_minus { -value } else { value };

            if let Some(d) = self.tokenizer.data().first().copied()
                && !Self::is_pdf_delimiter(d)
            {
                return Err(NumberError::MissingDelimiterAfterNumber(char::from(d)).into());
            }
            self.skip_whitespace();
            Ok(ObjectVariant::Real(number))
        } else {
            // 6. No decimal point: return as integer.
            self.skip_whitespace();
            let integer = if has_minus {
                integer_part
                    .checked_neg()
                    .ok_or(NumberError::NumericValueOverflow)?
            } else {
                integer_part
            };
            Ok(ObjectVariant::Integer(integer))
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
            // "0." is a valid PDF real number representing 0.0
            (b"0.", 0.0),
            (b"+0.", 0.0),
            (b"-0.", -0.0),
            (b"42.", 42.0),
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
            b".", b"-.",
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
