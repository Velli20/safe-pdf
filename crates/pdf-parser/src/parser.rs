use std::str::FromStr;

use crate::error::ParserError;
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};
use pdf_tokenizer::{PdfToken, Tokenizer};

/// Parses PDF objects from a borrowed byte slice.
pub struct PdfParser<'a> {
    /// The underlying tokenizer that drives byte-level reading.
    pub tokenizer: Tokenizer<'a>,
    /// Tracks the current recursive nesting depth while parsing.
    ///
    /// The parser increments this on entry to each object and decrements on exit.
    /// Callers should not mutate this field; it is public for testing purposes only.
    pub current_nesting_depth: usize,
}

impl<'a> From<&'a [u8]> for PdfParser<'a> {
    fn from(input: &'a [u8]) -> Self {
        PdfParser {
            tokenizer: Tokenizer::new(input),
            current_nesting_depth: 0,
        }
    }
}

impl PdfParser<'_> {
    /// Maximum nesting depth for PDF objects.
    const MAX_NESTING_DEPTH: usize = 32;

    /// Returns whether `c` is a PDF whitespace character (NUL, HT, LF, FF, CR, or SP).
    pub(crate) const fn is_pdf_whitespace(c: u8) -> bool {
        matches!(c, b'\0' | b'\t' | b'\n' | b'\x0C' | b'\r' | b' ')
    }

    /// Returns whether `c` is a PDF delimiter or whitespace character.
    pub(crate) const fn is_pdf_delimiter(c: u8) -> bool {
        if Self::is_pdf_whitespace(c) {
            return true;
        }
        matches!(
            c,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }

    /// Consumes an end-of-line marker from the input stream if one is present.
    ///
    /// Valid EOL markers are `\n`, `\r`, or `\r\n`. If none is present, does nothing.
    pub(crate) fn try_read_end_of_line_marker(&mut self) -> Result<(), ParserError> {
        if let Some(PdfToken::CarriageReturn) = self.tokenizer.peek() {
            self.tokenizer.read();
        }
        if let Some(PdfToken::NewLine) = self.tokenizer.peek() {
            self.tokenizer.read();
        }
        Ok(())
    }

    /// Advances past any whitespace characters at the current position.
    pub fn skip_whitespace(&mut self) {
        let _ = self.tokenizer.read_while_u8(Self::is_pdf_whitespace);
    }

    /// Skips whitespace and comments (`%` to end of line).
    ///
    /// Repeatedly advances past whitespace and `% ... EOL` comment sequences
    /// until a non-whitespace, non-comment token is reached.
    pub(crate) fn skip_whitespace_and_comments(&mut self) {
        loop {
            self.skip_whitespace();
            if let Some(PdfToken::Percent) = self.tokenizer.peek() {
                // Consume the '%' and everything up to EOL.
                self.tokenizer.read();
                let _ = self.tokenizer.read_while_u8(|c| c != b'\n' && c != b'\r');
                let _ = self.try_read_end_of_line_marker();
            } else {
                break;
            }
        }
    }

    /// Parses a PDF object at a specific byte offset in the input stream.
    ///
    /// Temporarily seeks to `position`, parses the object, then restores the original position.
    /// Useful for random access parsing when following cross-reference table entries.
    pub fn parse_object_at(
        &mut self,
        position: usize,
        objects: &dyn ObjectResolver,
    ) -> Result<ObjectVariant, ParserError> {
        let mark = self.tokenizer.position;
        self.tokenizer.position = position;
        let result = self.parse_object(objects);
        self.tokenizer.position = mark;

        result
    }

    /// Reads a sequence of ASCII digits and parses them into type `T`.
    ///
    /// Validates that a delimiter or decimal point follows the digits.
    /// Optionally skips trailing whitespace when `skip_whitespace` is true.
    pub fn read_number<T: FromStr>(&mut self, skip_whitespace: bool) -> Result<T, ParserError> {
        let number_bytes = self.tokenizer.read_while_u8(|b| b.is_ascii_digit());
        if number_bytes.is_empty() {
            return Err(ParserError::UnexpectedEndOfFile);
        }

        // number_bytes is guaranteed to be ASCII digits from the predicate above,
        // so from_utf8 always succeeds here.
        let number_str = std::str::from_utf8(number_bytes)
            .map_err(|_| ParserError::InvalidNumber("<non-UTF8 digit sequence>".to_owned()))?;
        let number = number_str
            .parse::<T>()
            .map_err(|_| ParserError::InvalidNumber(number_str.to_owned()))?;

        // Check that the following character after the number is a valid delimiter
        // or a dot (potential decimal number).
        if let Some(d) = self.tokenizer.data().first().copied()
            && !Self::is_pdf_delimiter(d)
            && d != b'.'
        {
            return Err(ParserError::MissingDelimiterAfterKeyword(d));
        }

        if skip_whitespace {
            self.skip_whitespace();
        }

        Ok(number)
    }

    /// Reads and validates a keyword literal from the input stream.
    ///
    /// Returns an error if the next bytes don't match `keyword` or if no delimiter follows.
    /// Consumes any trailing end-of-line marker after the keyword.
    pub fn read_keyword(&mut self, keyword: &[u8]) -> Result<(), ParserError> {
        let literal = self.tokenizer.read_excactly(keyword.len())?;
        if literal != keyword {
            return Err(ParserError::InvalidKeyword(
                String::from_utf8_lossy(keyword).to_string(),
                String::from_utf8_lossy(literal).to_string(),
            ));
        }

        if let Some(d) = self.tokenizer.data().first().copied()
            && !Self::is_pdf_delimiter(d)
        {
            return Err(ParserError::MissingDelimiterAfterKeyword(d));
        }

        // Consume trailing EOL if present (keywords in arrays/dicts may not have one).
        self.try_read_end_of_line_marker()
    }

    fn parse_object_internal(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<ObjectVariant, ParserError> {
        self.skip_whitespace();

        let Some(token) = self.tokenizer.peek() else {
            return Ok(ObjectVariant::EndOfFile);
        };

        let value = match token {
            PdfToken::Percent => ObjectVariant::Comment(self.parse_comment()?),
            PdfToken::DoublePercent => {
                self.tokenizer.read();
                const EOF_KEYWORD: &[u8] = b"EOF";

                // Read the keyword `EOF`.
                let literal = self.tokenizer.read_excactly(EOF_KEYWORD.len())?;
                if literal != EOF_KEYWORD {
                    return Err(ParserError::InvalidKeyword(
                        "EOF".to_string(),
                        String::from_utf8_lossy(literal).to_string(),
                    ));
                }
                return Ok(ObjectVariant::EndOfFile);
            }
            PdfToken::Alphabetic(t) => {
                if t == b't' {
                    // Try parsing as a trailer first.
                    let mark = self.tokenizer.position;
                    let value = self.parse_trailer(objects);
                    if let Ok(o) = value {
                        return Ok(ObjectVariant::Trailer(o));
                    }
                    // If that fails, reset and try parsing as a boolean.
                    self.tokenizer.position = mark;

                    ObjectVariant::Boolean(self.parse_boolean()?)
                } else if t == b'f' {
                    ObjectVariant::Boolean(self.parse_boolean()?)
                } else if t == b'n' {
                    self.parse_null_object()?;
                    ObjectVariant::Null
                } else if t == b'x' {
                    ObjectVariant::CrossReferenceTable(self.parse_cross_reference_table(objects)?)
                } else {
                    return Err(ParserError::InvalidToken(char::from(t)));
                }
            }
            PdfToken::DoubleLeftAngleBracket => {
                ObjectVariant::Dictionary(Box::new(self.parse_dictionary(objects)?))
            }
            PdfToken::LeftAngleBracket => ObjectVariant::HexString(self.parse_hex_string()?),
            PdfToken::Solidus => ObjectVariant::Name(self.parse_name()?),
            PdfToken::Number(_) => {
                // Numbers are ambiguous: could be an indirect object,
                // an indirect reference, or a plain number.
                let mark = self.tokenizer.position;

                // Try parsing as an indirect object first.
                if let Some(o) = self.parse_indirect_object(objects)? {
                    return Ok(o);
                }
                // If that fails, reset and try parsing as a number.
                self.tokenizer.position = mark;
                self.parse_number()?
            }
            PdfToken::Minus => self.parse_number()?,
            PdfToken::Plus => self.parse_number()?,
            PdfToken::Period => self.parse_number()?,
            PdfToken::LeftSquareBracket => ObjectVariant::Array(self.parse_array(objects)?),
            PdfToken::LeftParenthesis => ObjectVariant::LiteralString(self.parse_literal_string()?),
            token => {
                return Err(ParserError::UnexpectedTokenAt {
                    token: format!("{:?}", token),
                    position: self.tokenizer.position,
                });
            }
        };

        Ok(value)
    }

    /// Parses a single PDF object from the input stream at the current position.
    ///
    /// Dispatches to the appropriate sub-parser based on the next token. Enforces a maximum
    /// nesting depth to guard against deeply recursive structures.
    pub fn parse_object(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<ObjectVariant, ParserError> {
        // Prevent excessive nesting depth.
        if self.current_nesting_depth >= Self::MAX_NESTING_DEPTH {
            return Err(ParserError::NestingDepthExceeded);
        }
        self.current_nesting_depth = self.current_nesting_depth.saturating_add(1);
        let result = self.parse_object_internal(objects);
        self.current_nesting_depth = self.current_nesting_depth.saturating_sub(1);
        result
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use pdf_object::object_resolver::UnimplementedResolver;

    use super::*;

    #[test]
    fn test_unexpected_token() {
        let input = b"%PDF-1.3
 ";
        let mut parser = PdfParser::from(input.as_slice());
        let result = parser.parse_object(&UnimplementedResolver);
        assert!(result.is_ok());
    }
}
