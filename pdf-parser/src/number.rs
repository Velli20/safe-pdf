use pdf_object::number::Number;
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, error::ParserError};

impl ParseObject<Number> for PdfParser<'_> {
    /// Parses a PDF numeric object (integer or real) from the current position in the input stream.
    ///
    /// # Parsing Rules
    ///
    /// - Numeric objects can be either integers or real numbers.
    /// - Integers are represented as a sequence of digits (0-9) with an optional leading sign (`+` or `-`).
    /// - Examples: `123`, `-456`, `+789`.
    /// - Real numbers are represented as a sequence of digits, optionally followed by a decimal point (`.`)
    ///  and another sequence of digits.
    /// - A real number may also include an optional leading sign (`+` or `-`).
    /// - Examples: `3.14`, `-0.5`, `+123.456`.
    /// - Leading and trailing whitespace around the numeric object is ignored.
    ///
    /// # Returns
    ///
    /// A number object containing the parsed numeric value or an error  if the input is malformed or does not
    /// represent a valid numeric object.
    fn parse(&mut self) -> Result<Number, ParserError> {
        let mut has_minus = false;

        // 1. Check for optional sign.
        if let Some(PdfToken::Plus) = self.tokenizer.peek()? {
            self.tokenizer.read();
        } else if let Some(PdfToken::Minus) = self.tokenizer.peek()? {
            self.tokenizer.read();
            has_minus = true;
        }

        // 2. Parse leading digits (integral part).
        let digits = self.read_number::<i64>(ParserError::InvalidNumber)?;

        // 3. Check for decimal point
        if let Some(PdfToken::Period) = self.tokenizer.peek()? {
            self.tokenizer.read();
            // 4. Parse fractional part.
            let fraction = self.read_number::<i64>(ParserError::InvalidNumber)?;
            // 5. Combine integral and fractional parts.
            let number_str = if has_minus {
                format!("-{}.{}", digits, fraction)
            } else {
                format!("{}.{}", digits, fraction)
            };
            // 6. Convert to f64.
            let number = number_str
                .parse::<f64>()
                .map_err(|_| ParserError::InvalidNumber)?;

            self.skip_whitespace();
            Ok(Number::new(number))
        } else {
            // 7. No decimal point, parse as integer.
            self.skip_whitespace();
            if has_minus {
                Ok(Number::new(-digits))
            } else {
                Ok(Number::new(digits))
            }
        }
    }
}

#[cfg(test)]
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
            let result: Number = parser.parse().unwrap();
            assert_eq!(result, Number::new(expected));
        }
    }

    #[test]
    fn test_parse_number_valid_floats() {
        let valid_inputs: Vec<(&[u8], f64)> = vec![
            (b"123.456 ", 123.456),
            (b"-0.789 ", -0.789),
            (b"+3.14 ", 3.14),
            (b"0.0 ", 0.0),
        ];

        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let result: Number = parser.parse().unwrap();
            assert_eq!(result, Number::new(expected));
        }
    }

    #[test]
    fn test_parse_number_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![
            b"--42",  // double minus
            b"++17",  // double plus
            b"+-5",   // invalid combination
            b"4,200", // comma not allowed
            b"123abc ", // Mixed numeric and non-numeric characters
                      //      b"--123 ",        // Invalid double minus
                      //     b"123..456 ",     // Invalid double decimal point
                      //    b"123.456.789 ",  // Multiple decimal points
        ];

        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result: Result<Number, ParserError> = parser.parse();
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
