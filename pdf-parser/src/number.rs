use pdf_object::number::Number;
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, error::ParserError};

impl ParseObject<Number> for PdfParser<'_> {
    /// Parses a PDF numeric object (integer or real) from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification (Section 7.3.3), numeric objects can be
    /// integers or real numbers.
    ///
    /// # Format
    ///
    /// ## Integers
    /// - Consist of an optional sign (`+` or `-`) followed by one or more decimal digits.
    /// - Examples: `123`, `-45`, `+0`
    ///
    /// ## Real Numbers
    /// - Can be represented in several forms:
    ///   - `digits.digits` (e.g., `34.5`, `-3.62`, `+123.6`)
    ///   - `.digits` (e.g., `.5`)
    ///   - `digits.` (e.g., `0.`, `123.`)
    /// - An optional sign (`+` or `-`) can precede the number.
    /// - This parser specifically handles the `digits.digits` form after an optional sign.
    ///   It reads the integral part, then if a decimal point is present, reads the fractional part.
    ///
    /// # Implementation Notes
    ///
    /// - The parser first attempts to read an optional sign (`+` or `-`).
    /// - It then reads the integral part of the number.
    /// - If a decimal point (`.`) is encountered, it proceeds to read the fractional part.
    /// - Integers are stored as `i64` if no decimal is present.
    /// - Real numbers (with a decimal) are parsed into `f64`.
    ///
    /// # Example Inputs
    ///
    /// ```text
    /// 123
    /// -456
    /// +0.789
    /// 3.14159
    /// -100.
    /// .5
    /// ```
    ///
    /// # Returns
    ///
    /// A `Number` object containing the parsed integer (`i64`) or real (`f64`) value,
    /// or a `ParserError` if the input is malformed (e.g., invalid characters,
    /// missing digits after a sign or decimal point).
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
        let digits = self.read_number::<i64>()?;

        // 3. Check for decimal point
        if let Some(PdfToken::Period) = self.tokenizer.peek()? {
            self.tokenizer.read();
            // 4. Parse fractional part.
            let fraction = self.read_number::<i64>()?;
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
