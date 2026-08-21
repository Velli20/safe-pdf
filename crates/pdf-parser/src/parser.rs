use std::str::FromStr;

use crate::error::ParserError;
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};
use pdf_tokenizer::{PdfToken, Tokenizer};

/// Parses PDF objects from a borrowed byte slice.
pub struct PdfParser<'a> {
    /// The underlying tokenizer that drives byte-level reading.
    pub(crate) tokenizer: Tokenizer<'a>,
    /// Tracks the current recursive nesting depth while parsing.
    ///
    /// The parser increments this on entry to each object and decrements on exit.
    current_nesting_depth: usize,
}

impl<'a> From<&'a [u8]> for PdfParser<'a> {
    fn from(input: &'a [u8]) -> Self {
        PdfParser {
            tokenizer: Tokenizer::new(input),
            current_nesting_depth: 0,
        }
    }
}

impl<'a> PdfParser<'a> {
    /// Maximum nesting depth for PDF objects.
    const MAX_NESTING_DEPTH: usize = 32;

    /// Creates an independent parser positioned at an absolute byte offset.
    ///
    /// The returned parser borrows the same input, starts with a fresh nesting
    /// depth, and does not share cursor state with this parser.
    pub fn at_offset(&self, offset: usize) -> Result<Self, ParserError> {
        if offset > self.tokenizer.input.len() {
            return Err(ParserError::InvalidOffset {
                offset,
                input_length: self.tokenizer.input.len(),
            });
        }

        let mut parser = Self::from(self.tokenizer.input);
        parser.tokenizer.position = offset;
        Ok(parser)
    }

    /// Returns the current absolute byte offset.
    pub const fn position(&self) -> usize {
        self.tokenizer.position
    }

    /// Returns the unconsumed input beginning at the current cursor.
    pub fn remaining_input(&self) -> &[u8] {
        self.tokenizer.data()
    }

    /// Returns the next raw byte without consuming it.
    pub fn peek_byte(&self) -> Option<u8> {
        self.tokenizer.peek_byte()
    }

    /// Consumes and returns the next raw byte.
    pub fn read_byte(&mut self) -> Option<u8> {
        self.tokenizer.next_byte()
    }

    /// Returns whether `c` is a PDF whitespace character (NUL, HT, LF, FF, CR, or SP).
    pub const fn is_pdf_whitespace(c: u8) -> bool {
        matches!(c, b'\0' | b'\t' | b'\n' | b'\x0C' | b'\r' | b' ')
    }

    /// Returns whether `c` is a PDF delimiter or whitespace character.
    pub const fn is_pdf_delimiter(c: u8) -> bool {
        if Self::is_pdf_whitespace(c) {
            return true;
        }
        matches!(
            c,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }

    /// Returns whether `c` is a regular PDF character.
    ///
    /// Regular characters are any bytes that are neither whitespace nor delimiters.
    pub const fn is_pdf_regular_character(c: u8) -> bool {
        !Self::is_pdf_delimiter(c)
    }

    /// Consumes exactly one end-of-line marker from the input stream if one is present.
    ///
    /// Valid EOL sequences are `\r\n` (CRLF), `\r` (CR), or `\n` (LF), consumed in that
    /// priority order. If no EOL marker is present at the current position, does nothing.
    pub fn try_read_end_of_line_marker(&mut self) {
        match self.tokenizer.peek_byte() {
            Some(b'\r') => {
                let _ = self.tokenizer.read();
                // Consume a following LF to handle the CRLF sequence.
                if matches!(self.tokenizer.peek_byte(), Some(b'\n')) {
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

    /// Skips from the current position to the end of the current line.
    ///
    /// Call after the leading `%` (or `%%`) of a comment has already been consumed.
    fn skip_comment_body(&mut self) {
        let _ = self.tokenizer.read_while_u8(|c| c != b'\n' && c != b'\r');
        self.try_read_end_of_line_marker();
    }

    /// Returns `true` if the current position is at a `%%EOF` marker.
    ///
    /// Uses [`read_keyword`] for the `EOF` check (including the trailing
    /// delimiter requirement) and always restores the original position.
    fn is_at_eof_marker(&mut self) -> bool {
        let mark = self.tokenizer.position;
        let is_eof = matches!(self.tokenizer.peek(), Some(PdfToken::DoublePercent)) && {
            self.tokenizer.read();
            self.read_keyword(b"EOF").is_ok()
        };
        self.tokenizer.position = mark;
        is_eof
    }

    /// Skips whitespace and comments (`%` to end of line).
    ///
    /// Repeatedly advances past whitespace and `% … EOL` comment sequences
    /// until a non-whitespace, non-comment token is reached.
    ///
    /// Per the PDF spec (ISO 32000-1:2008 §7.2.3), a comment runs from `%`
    /// to (but not including) the next CR, LF, or CRLF end-of-line sequence,
    /// which is then consumed.  The `%%EOF` marker is the sole exception: it
    /// is *not* a comment and is left unconsumed for the caller to handle.
    pub fn skip_whitespace_and_comments(&mut self) {
        loop {
            self.skip_whitespace();
            match self.tokenizer.peek() {
                Some(PdfToken::Percent) => {
                    self.tokenizer.read();
                    self.skip_comment_body();
                }
                Some(PdfToken::DoublePercent) if !self.is_at_eof_marker() => {
                    self.tokenizer.read();
                    self.skip_comment_body();
                }
                _ => break,
            }
        }
    }

    /// Consumes a `%%EOF` marker as a line comment when parsing embedded grammars.
    ///
    /// Normal PDF parsing must preserve `%%EOF`, but embedded streams such as
    /// CMaps can contain it as ordinary PostScript-style comment text.
    pub fn skip_eof_marker_as_comment(&mut self) -> bool {
        if !self.is_at_eof_marker() {
            return false;
        }

        self.tokenizer.read();
        self.skip_comment_body();
        true
    }

    fn read_regular_character_token(&mut self) -> Result<&'a [u8], ParserError> {
        match self.tokenizer.peek_byte() {
            Some(b) if Self::is_pdf_regular_character(b) => {
                Ok(self.tokenizer.read_while_u8(Self::is_pdf_regular_character))
            }
            Some(b) => Err(ParserError::InvalidToken(char::from(b))),
            None => Err(ParserError::UnexpectedEndOfFile),
        }
    }

    /// Reads a PDF operator name from the current parser position.
    ///
    /// Content stream operators are PDF keywords, so this reads a single token
    /// consisting of consecutive regular PDF characters.
    pub fn read_operator_name(&mut self) -> Result<&'a [u8], ParserError> {
        self.read_regular_character_token()
    }

    /// Reads a sequence of ASCII digits and parses them into type `T`.
    ///
    /// Best-effort parsing intentionally stops at the first non-digit byte instead of enforcing a
    /// trailing token delimiter. Higher-level parsers decide whether the remaining bytes form a
    /// meaningful continuation.
    /// Optionally skips trailing whitespace when `skip_whitespace` is true.
    pub fn read_number<T: FromStr>(&mut self, skip_whitespace: bool) -> Result<T, ParserError> {
        let number_bytes = self.tokenizer.read_while_u8(|b| b.is_ascii_digit());
        if number_bytes.is_empty() {
            return match self.tokenizer.data().first().copied() {
                Some(byte) => Err(ParserError::UnexpectedTokenAt {
                    token: String::from_utf8_lossy(&[byte]).into_owned(),
                    position: self.tokenizer.position,
                }),
                None => Err(ParserError::UnexpectedEndOfFile),
            };
        }

        // number_bytes is guaranteed to be ASCII digits from the predicate above,
        // so from_utf8 always succeeds here.
        let number_str = std::str::from_utf8(number_bytes)
            .map_err(|_| ParserError::InvalidNumber("<non-UTF8 digit sequence>".to_owned()))?;
        let number = number_str
            .parse::<T>()
            .map_err(|_| ParserError::InvalidNumber(number_str.to_owned()))?;

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
        self.read_keyword_with_optional_eol(keyword, true)
    }

    /// Reads and validates a keyword literal from the input stream.
    ///
    /// When `consume_trailing_eol` is `false`, the parser leaves a following CR, LF,
    /// or CRLF sequence untouched so callers can handle that boundary themselves.
    pub(crate) fn read_keyword_with_optional_eol(
        &mut self,
        keyword: &[u8],
        consume_trailing_eol: bool,
    ) -> Result<(), ParserError> {
        let keyword_start = self.tokenizer.position;
        let literal = self.read_regular_character_token()?;

        if literal != keyword {
            if literal.starts_with(keyword)
                && let Some(found) = literal.get(keyword.len()).copied()
            {
                return Err(ParserError::MissingDelimiterAfterKeyword {
                    keyword: String::from_utf8_lossy(keyword).into_owned(),
                    found,
                    position: keyword_start.saturating_add(keyword.len()),
                });
            }

            return Err(ParserError::InvalidKeyword(
                String::from_utf8_lossy(keyword).to_string(),
                String::from_utf8_lossy(literal).to_string(),
            ));
        }

        if consume_trailing_eol {
            // Consume trailing EOL if present (keywords in arrays/dicts may not have one).
            self.try_read_end_of_line_marker();
        }
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
            PdfToken::Number(_) => self.parse_number_or_reference()?,
            PdfToken::Minus => self.parse_number()?,
            PdfToken::Plus => self.parse_number()?,
            PdfToken::Period => self.parse_number()?,
            PdfToken::LeftSquareBracket => ObjectVariant::Array(self.parse_array(objects)?),
            PdfToken::LeftParenthesis => ObjectVariant::LiteralString(self.parse_literal_string()?),
            token => {
                return Err(ParserError::UnexpectedTokenAt {
                    token: format!("{token:?}"),
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
    fn at_offset_creates_an_independent_parser() {
        let parser = PdfParser::from(b"0 1".as_slice());
        let mut fork = parser.at_offset(2).unwrap();

        assert_eq!(fork.parse_number().unwrap(), ObjectVariant::Integer(1));
        assert_eq!(parser.position(), 0);
        assert_eq!(fork.position(), 3);
    }

    #[test]
    fn at_offset_accepts_end_of_input() {
        let input = b"data";
        let parser = PdfParser::from(input.as_slice());
        let fork = parser.at_offset(input.len()).unwrap();

        assert_eq!(fork.position(), input.len());
        assert_eq!(fork.peek_byte(), None);
    }

    #[test]
    fn at_offset_rejects_positions_beyond_input() {
        let parser = PdfParser::from(b"data".as_slice());

        assert_eq!(
            parser.at_offset(5).err().unwrap(),
            ParserError::InvalidOffset {
                offset: 5,
                input_length: 4,
            }
        );
    }

    #[test]
    fn test_unexpected_token() {
        let input = b"%PDF-1.3
 ";
        let mut parser = PdfParser::from(input.as_slice());
        let result = parser.parse_object(&PassthroughResolver);
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_number_stops_at_first_non_digit_for_best_effort() {
        let mut parser = PdfParser::from(b"123abc".as_slice());

        let number = parser.read_number::<usize>(false).unwrap();

        assert_eq!(number, 123);
        assert_eq!(parser.tokenizer.data(), b"abc");
    }

    #[test]
    fn test_read_number_returns_error_on_non_digit_input() {
        let mut parser = PdfParser::from(b"%123".as_slice());

        let error = parser.read_number::<usize>(false).unwrap_err();

        assert_eq!(
            error,
            ParserError::UnexpectedTokenAt {
                token: "%".to_string(),
                position: 0,
            }
        );
    }

    #[test]
    fn test_read_keyword_error_reports_keyword_and_offset() {
        let mut parser = PdfParser::from(b"truefalse".as_slice());

        let error = parser.read_keyword(b"true").unwrap_err();

        assert_eq!(
            error,
            ParserError::MissingDelimiterAfterKeyword {
                keyword: "true".to_owned(),
                found: b'f',
                position: 4,
            }
        );
    }

    #[test]
    fn test_read_operator_name_reads_complete_regular_character_token() {
        let cases = [
            b"q ".as_slice(),
            b"T* ".as_slice(),
            b"d1 ".as_slice(),
            b"' ".as_slice(),
            b"\" ".as_slice(),
        ];

        for input in cases {
            let mut parser = PdfParser::from(input);
            let operator = parser.read_operator_name().unwrap();
            let expected = input
                .get(..input.len().saturating_sub(1))
                .expect("operator test input should contain trailing whitespace");

            assert_eq!(operator, expected);
            assert_eq!(parser.tokenizer.data(), b" ");
        }
    }

    #[test]
    fn test_read_operator_name_rejects_non_regular_character_start() {
        for input in [
            b"".as_slice(),
            b"/Name".as_slice(),
            b"(text)".as_slice(),
            b" value".as_slice(),
        ] {
            let mut parser = PdfParser::from(input);
            assert!(parser.read_operator_name().is_err());
        }
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

        /// `%%` is tokenised as `DoublePercent`, not `Percent`, but `%%EOF`
        /// is specifically detected and preserved for the caller.
        #[test]
        fn double_percent_eof_not_skipped() {
            let mut p = PdfParser::from(b"%%EOF".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"%%EOF");
        }

        /// `%%EOF` followed by a newline is still preserved.
        #[test]
        fn double_percent_eof_with_trailing_newline_not_skipped() {
            let mut p = PdfParser::from(b"%%EOF\n".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"%%EOF\n");
        }

        /// `%%%` followed by comment text is an ordinary comment and must be skipped.
        #[test]
        fn triple_percent_comment_skipped() {
            let input = b"%%% FType3A stroked red 'rect' = 97 or 'a'\ncontent";
            let mut p = PdfParser::from(input.as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        /// `%%` followed by non-`EOF` text is a comment and must be skipped.
        #[test]
        fn double_percent_non_eof_comment_skipped() {
            let mut p = PdfParser::from(b"%% this is a comment\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        /// `%%EOFOO` does not match the `%%EOF` marker (no delimiter after `EOF`).
        #[test]
        fn double_percent_eofoo_is_comment() {
            let mut p = PdfParser::from(b"%%EOFOO\ncontent".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"content");
        }

        /// `%%` followed by nothing (bare `%%` at end of input) is consumed as a comment.
        #[test]
        fn bare_double_percent_at_eof() {
            let mut p = PdfParser::from(b"%%".as_slice());
            p.skip_whitespace_and_comments();
            assert_eq!(remaining(&p), b"");
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
