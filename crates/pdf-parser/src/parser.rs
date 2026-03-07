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

    /// Consumes exactly one end-of-line marker from the input stream if one is present.
    ///
    /// Valid EOL sequences are `\r\n` (CRLF), `\r` (CR), or `\n` (LF), consumed in that
    /// priority order. If no EOL marker is present at the current position, does nothing.
    pub(crate) fn try_read_end_of_line_marker(&mut self) {
        match self.tokenizer.data().first().copied() {
            Some(b'\r') => {
                let _ = self.tokenizer.read();
                // Consume a following LF to handle the CRLF sequence.
                if matches!(self.tokenizer.data().first().copied(), Some(b'\n')) {
                    let _ = self.tokenizer.read();
                }
            }
            Some(b'\n') => {
                let _ = self.tokenizer.read();
            }
            _ => {}
        }
    }

    /// Advances past any whitespace characters at the current position.
    pub fn skip_whitespace(&mut self) {
        let _ = self.tokenizer.read_while_u8(Self::is_pdf_whitespace);
    }

    /// Skips whitespace and comments (`%` to end of line).
    ///
    /// Repeatedly advances past whitespace and `% ... EOL` comment sequences
    /// until a non-whitespace, non-comment token is reached.
    ///
    /// Per the PDF spec, a comment runs from `%` to (but not including) the
    /// next CR, LF, or CRLF end-of-line sequence, which is then consumed.
    /// The `%%` token is not treated as a comment start here; it is handled
    /// separately as the `%%EOF` end-of-file marker.
    pub fn skip_whitespace_and_comments(&mut self) {
        loop {
            self.skip_whitespace();
            if let Some(PdfToken::Percent) = self.tokenizer.peek() {
                // Consume the '%' and everything up to (not including) the EOL.
                self.tokenizer.read();
                let _ = self.tokenizer.read_while_u8(|c| c != b'\n' && c != b'\r');
                self.try_read_end_of_line_marker();
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
        let literal = self.tokenizer.read_exactly(keyword.len())?;
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
        self.try_read_end_of_line_marker();
        Ok(())
    }

    fn parse_object_internal(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<ObjectVariant, ParserError> {
        self.skip_whitespace_and_comments();

        let Some(token) = self.tokenizer.peek() else {
            return Ok(ObjectVariant::EndOfFile);
        };

        let value = match token {
            PdfToken::DoublePercent => {
                self.tokenizer.read();
                const EOF_KEYWORD: &[u8] = b"EOF";

                self.read_keyword(EOF_KEYWORD)?;
                return Ok(ObjectVariant::EndOfFile);
            }
            PdfToken::Alphabetic(t) => {
                const BOOLEAN_LITERAL_TRUE: &[u8] = b"true";
                const BOOLEAN_LITERAL_FALSE: &[u8] = b"false";
                const NULL_LITERAL: &[u8] = b"null";

                match t {
                    b't' => {
                        self.read_keyword(BOOLEAN_LITERAL_TRUE)?;
                        ObjectVariant::Boolean(true)
                    }
                    b'f' => {
                        self.read_keyword(BOOLEAN_LITERAL_FALSE)?;
                        ObjectVariant::Boolean(false)
                    }
                    b'n' => {
                        self.read_keyword(NULL_LITERAL)?;
                        ObjectVariant::Null
                    }
                    b'x' => ObjectVariant::CrossReferenceTable(
                        self.parse_cross_reference_table(objects)?,
                    ),
                    other => {
                        return Err(ParserError::InvalidToken(char::from(other)));
                    }
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
    use pdf_object::object_resolver::PassthroughResolver;

    use super::*;

    #[test]
    fn test_unexpected_token() {
        let input = b"%PDF-1.3
 ";
        let mut parser = PdfParser::from(input.as_slice());
        let result = parser.parse_object(&PassthroughResolver);
        assert!(result.is_ok());
    }

    mod skip_whitespace_and_comments {
        use super::*;

        fn remaining<'a>(p: &'a PdfParser<'a>) -> &'a [u8] {
            p.tokenizer.data()
        }

        #[test]
        fn empty_input() {
            let mut p = PdfParser::from(b"".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"");
        }

        #[test]
        fn no_whitespace_or_comments() {
            let mut p = PdfParser::from(b"content".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        #[test]
        fn spaces_and_tabs_only() {
            let mut p = PdfParser::from(b"   \t   content".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        #[test]
        fn newlines_only() {
            let mut p = PdfParser::from(b"\n\r\r\n\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        #[test]
        fn comment_terminated_by_lf() {
            let mut p = PdfParser::from(b"% comment\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        #[test]
        fn comment_terminated_by_cr() {
            let mut p = PdfParser::from(b"% comment\rcontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        #[test]
        fn comment_terminated_by_crlf() {
            let mut p = PdfParser::from(b"% comment\r\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        /// A comment with no trailing newline runs to end-of-file.
        #[test]
        fn comment_at_end_of_file() {
            let mut p = PdfParser::from(b"% no newline".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"");
        }

        #[test]
        fn empty_comment_body() {
            let mut p = PdfParser::from(b"%\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        #[test]
        fn multiple_consecutive_comments() {
            let mut p = PdfParser::from(b"% first\n% second\n% third\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        #[test]
        fn whitespace_before_comment() {
            let mut p = PdfParser::from(b"   % comment\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        #[test]
        fn whitespace_after_comment() {
            let mut p = PdfParser::from(b"% comment\n   content".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        #[test]
        fn mixed_whitespace_and_comments() {
            let input = b"\n% line 1\n\n% line 2\r\n  content";
            let mut p = PdfParser::from(input.as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        /// `%` inside a comment body does not start a nested comment.
        #[test]
        fn percent_inside_comment_body() {
            let mut p = PdfParser::from(b"% 50% off\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        /// High bytes (e.g. binary PDF marker `%âãÏÓ`) are treated as comment body.
        #[test]
        fn high_byte_chars_in_comment() {
            let mut p = PdfParser::from(b"% \xe2\xe3\xcf\xd3\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        /// NUL bytes inside a comment body are consumed without error.
        #[test]
        fn nul_byte_in_comment_body() {
            let mut p = PdfParser::from(b"% text\x00more\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        /// `%%` is tokenised as `DoublePercent`, not `Percent`, so it is NOT
        /// consumed as a comment — it remains for the caller (e.g. `%%EOF` detection).
        #[test]
        fn double_percent_not_skipped() {
            let mut p = PdfParser::from(b"%%EOF".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"%%EOF");
        }

        /// CRLF is consumed as a single two-byte EOL sequence.
        #[test]
        fn crlf_consumed_as_single_eol() {
            let mut p = PdfParser::from(b"% first\r\n% second\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        /// Whitespace-only input spanning all PDF whitespace character values.
        #[test]
        fn all_pdf_whitespace_chars() {
            // NUL, HT, LF, FF, CR, SP
            let mut p = PdfParser::from(b"\x00\t\n\x0C\r content".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }
    }

    #[test]
    fn test_parse_boolean_valid() {
        let valid_inputs: Vec<(&[u8], bool)> = vec![
            (b"true ", true),
            (b"false ", false),
            (b"true\n", true),
            (b"false\t", false),
        ];

        for (input, expected) in valid_inputs {
            let mut parser = PdfParser::from(input);
            let value = parser.parse_object(&PassthroughResolver).unwrap();
            assert_eq!(
                value,
                ObjectVariant::Boolean(expected),
                "Failed to parse `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn test_parse_boolean_invalid() {
        let invalid_inputs: Vec<&[u8]> = vec![b"tru ", b"fals ", b"truefalse", b"false123"];

        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_object(&PassthroughResolver);
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn test_null_object() {
        let valid_inputs: Vec<&[u8]> = vec![
            b"null\n",
            b"null\t",
            b"null ",
            b"null<",
            b"null>",
            b"null[",
            b"null]",
            b"null{",
            b"null}",
            b"null(abc)",
        ];

        for input in valid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_object(&PassthroughResolver);
            assert_eq!(
                result,
                Ok(ObjectVariant::Null),
                "Failed to parse `{}`",
                String::from_utf8_lossy(input)
            );
        }
        let invalid_inputs: Vec<&[u8]> = vec![
            b"nullabc\n",
            b"null123\n",
            b"nulla",
            b"nullobj\n",
            b"nullobj<",
            b"nullobj>",
            b"nullobj[",
            b"nullobj]",
            b"nullobj{",
            b"nullobj}",
        ];
        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_object(&PassthroughResolver);
            assert!(
                result.is_err(),
                "Expected error for invalid input `{}`",
                String::from_utf8_lossy(input)
            );
        }
    }
}
