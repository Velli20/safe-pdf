use pdf_object_reader::object_variant::ObjectVariant;

use crate::{error::ParserError, parser::PdfParser};

impl PdfParser<'_> {
    /// Parses a PDF numeric object (integer or real) from the current position in the input stream.
    ///
    /// # Returns
    ///
    /// A `Number` object containing the parsed integer (`i64`) or real (`f64`) value,
    /// or a `ParserError` on failure.
    pub fn parse_number(&mut self) -> Result<ObjectVariant, ParserError> {
        // Keep the starting offset so the complete numeric lexeme can be borrowed after
        // scanning. Nothing is copied while identifying the token.
        let start = self.tokenizer.position;

        // A PDF number may begin with exactly one optional sign. Consume it separately
        // from the digit runs so a second sign remains visible and causes parsing to fail.
        let has_sign = if matches!(self.tokenizer.peek_byte(), Some(b'+' | b'-')) {
            let _ = self.tokenizer.next_byte();
            true
        } else {
            false
        };

        // Scan the integral digits. `read_while_u8` stops at the first non-digit, which
        // preserves best-effort parsing of malformed glued tokens such as `61endobj`:
        // only `61` belongs to the number and `endobj` remains for the next parser step.
        let mut has_digits = !self
            .tokenizer
            .read_while_u8(|byte| byte.is_ascii_digit())
            .is_empty();

        // PDF real numbers contain one decimal point and permit digits on only one side
        // (`.5` and `42.` are both valid). Consume at most one point, then scan the
        // fractional digits. Recording `is_real` also selects the target Rust type below.
        let is_real = if matches!(self.tokenizer.peek_byte(), Some(b'.')) {
            let _ = self.tokenizer.next_byte();
            has_digits |= !self
                .tokenizer
                .read_while_u8(|byte| byte.is_ascii_digit())
                .is_empty();
            true
        } else {
            false
        };

        // Some malformed PDFs use a standalone `+` or `-` as a zero-valued text-array
        // adjustment. Retain that recovery only when another, non-sign object follows.
        // A sign at EOF remains truncated input, while a second sign is left to fail as
        // an invalid numeric lexeme instead of being silently accepted as zero.
        if !has_digits && !is_real && has_sign {
            match self.tokenizer.peek_byte() {
                Some(b'+' | b'-') => {}
                Some(_) => {
                    self.skip_whitespace();
                    return Ok(ObjectVariant::Integer(0));
                }
                None => return Err(ParserError::UnexpectedEndOfFile),
            }
        }

        // The tokenizer cursor now marks the exclusive end of the lexeme:
        // `[optional sign][integral digits][optional point][fractional digits]`.
        // Borrow that exact range from the original input so successful parsing does not
        // allocate. `get` keeps this safe even if an externally modified tokenizer cursor
        // violates its normal bounds invariant.
        let number_bytes = self
            .tokenizer
            .input
            .get(start..self.tokenizer.position)
            .unwrap_or(&[]);

        // An empty range means the caller invoked number parsing at a byte that cannot
        // begin a number. Report the byte without consuming it so higher-level recovery
        // can decide how to proceed.
        if number_bytes.is_empty() {
            return match self.tokenizer.peek_byte() {
                Some(byte) => Err(ParserError::UnexpectedTokenAt {
                    token: String::from_utf8_lossy(&[byte]).into_owned(),
                    position: self.tokenizer.position,
                }),
                None => Err(ParserError::UnexpectedEndOfFile),
            };
        }

        // Every consumed byte is ASCII by construction, so `from_utf8_lossy` returns a
        // borrowed string for valid parser state. It also gives the error path a safe,
        // printable owned representation without unsafe UTF-8 assumptions.
        let number_str = String::from_utf8_lossy(number_bytes);

        // Parse the entire lexeme with the standard library. This handles the sign and
        // overflow checks for integers and provides correctly rounded decimal-to-f64
        // conversion for reals. Both conversion failures use one public numeric error.
        let number = if is_real {
            number_str
                .parse::<f64>()
                .map(ObjectVariant::Real)
                .map_err(|_| ParserError::InvalidNumber(number_str.into_owned()))?
        } else {
            number_str
                .parse::<i64>()
                .map(ObjectVariant::Integer)
                .map_err(|_| ParserError::InvalidNumber(number_str.into_owned()))?
        };

        // A parsed PDF object consumes trailing PDF whitespace, matching the other
        // primitive parsers while leaving the next non-whitespace token untouched.
        self.skip_whitespace();
        Ok(number)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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
            b"--42", // double minus
            b"++17", // double plus
            b"+-5",  // invalid combination
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

    #[test]
    fn test_parse_number_treats_bare_sign_as_zero_and_keeps_next_token() {
        for (input, trailing) in [
            (b"-(".as_slice(), b"(".as_slice()),
            (b"+(".as_slice(), b"(".as_slice()),
        ] {
            let mut parser = PdfParser::from(input);

            let result = parser.parse_number().unwrap();

            assert_eq!(result, ObjectVariant::Integer(0));
            assert_eq!(parser.tokenizer.data(), trailing);
        }
    }

    #[test]
    fn test_parse_number_keeps_trailing_bytes_for_best_effort() {
        let mut parser = PdfParser::from(b"61endobj".as_slice());

        let result = parser.parse_number().unwrap();

        assert_eq!(result, ObjectVariant::Integer(61));
        assert_eq!(parser.tokenizer.data(), b"endobj");
    }

    #[test]
    fn test_parse_number_integer_boundaries_and_overflow_positions() {
        for (input, expected) in [
            (b"9223372036854775807".as_slice(), i64::MAX),
            (b"-9223372036854775807".as_slice(), -i64::MAX),
            (b"-9223372036854775808".as_slice(), i64::MIN),
        ] {
            let mut parser = PdfParser::from(input);

            assert_eq!(
                parser.parse_number().unwrap(),
                ObjectVariant::Integer(expected)
            );
            assert_eq!(parser.position(), input.len());
        }

        for input in [
            b"9223372036854775808".as_slice(),
            b"-9223372036854775809".as_slice(),
        ] {
            let mut parser = PdfParser::from(input);

            assert_eq!(
                parser.parse_number().unwrap_err(),
                ParserError::InvalidNumber(String::from_utf8_lossy(input).into_owned())
            );
            assert_eq!(parser.position(), input.len());
        }
    }

    #[test]
    fn test_parse_number_uses_standard_real_bit_patterns() {
        let expected = "12.12345678901234567890".parse::<f64>().unwrap();
        let mut parser = PdfParser::from(b"12.12345678901234567890".as_slice());

        let ObjectVariant::Real(actual) = parser.parse_number().unwrap() else {
            panic!("expected a real number");
        };

        assert_eq!(actual.to_bits(), expected.to_bits());

        let mut negative_zero_parser = PdfParser::from(b"-0.".as_slice());
        let ObjectVariant::Real(negative_zero) = negative_zero_parser.parse_number().unwrap()
        else {
            panic!("expected a real number");
        };
        assert_eq!(negative_zero.to_bits(), (-0.0_f64).to_bits());
    }

    #[test]
    fn test_parse_number_reports_invalid_lexeme_and_preserves_cursor_positions() {
        let mut invalid = PdfParser::from(b"--42".as_slice());
        assert_eq!(
            invalid.parse_number().unwrap_err(),
            ParserError::InvalidNumber("-".to_owned())
        );
        assert_eq!(invalid.position(), 1);

        let mut whitespace = PdfParser::from(b"1\0\t\n\x0c\r 2".as_slice());
        assert_eq!(
            whitespace.parse_number().unwrap(),
            ObjectVariant::Integer(1)
        );
        assert_eq!(whitespace.remaining_input(), b"2");
    }
}
