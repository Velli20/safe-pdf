use std::{rc::Rc, str::FromStr};

use crate::{error::ParserError, traits::HeaderParser};
use pdf_object::{ObjectVariant, cross_reference_table::CrossReferenceTable};
use pdf_tokenizer::{PdfToken, Tokenizer};

use crate::traits::{
    ArrayParser, BooleanParser, CommentParser, CrossReferenceTableParser, DictionaryParser,
    HexStringParser, IndirectObjectParser, LiteralStringParser, NameParser, NullObjectParser,
    NumberParser, TrailerParser,
};

/// Represents a PDF object parser that handles parsing various
/// PDF objects from an input stream.
pub struct PdfParser<'a> {
    /// The tokenizer used for parsing the PDF input stream.
    pub tokenizer: Tokenizer<'a>,
    /// Current nesting depth of PDF objects being parsed.
    /// This is used to prevent excessive recursion and potential stack overflows.
    pub current_nesting_depth: usize,
    /// Optional cross-reference table parsed from the document, if available.
    pub xref_table: Option<CrossReferenceTable>,
}

impl<'a> From<&'a [u8]> for PdfParser<'a> {
    fn from(input: &'a [u8]) -> Self {
        PdfParser {
            tokenizer: Tokenizer::new(input),
            current_nesting_depth: 0,
            xref_table: None,
        }
    }
}

impl PdfParser<'_> {
    /// Maximum nesting depth for PDF objects.
    const MAX_NESTING_DEPTH: usize = 32;

    /// Checks if a character is a whitespace according to PDF 1.7 spec (Section 7.2.2).
    /// Whitespace characters are defined as:
    /// - Null (NUL) - `0x00` (`b'\0'`)
    /// - Horizontal Tab (HT) - `0x09` (`b'\t'`)
    /// - Line Feed (LF) - `0x0A` (`b'\n'`)
    /// - Form Feed (FF) - `0x0C` (`b'\x0C'`)
    /// - Carriage Return (CR) - `0x0D` (`b'\r'`)
    /// - Space (SP) - `0x20` (`b' '`)
    pub(crate) const fn is_pdf_whitespace(c: u8) -> bool {
        matches!(
            c,
            // Whitespace characters (Common ones)
            b' ' | b'\t' | b'\n' | b'\r' | b'\x0C'
        )
    }

    /// Checks if a character is a PDF delimiter according to PDF 1.7 spec (Section 7.2.2).
    /// Whitespace characters (space, tab, newline, etc.) also act as delimiters.
    pub(crate) const fn is_pdf_delimiter(c: u8) -> bool {
        if Self::is_pdf_whitespace(c) {
            return true;
        }
        // Delimiter characters
        matches!(
            c,
            // Delimiter characters
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }

    /// Reads an end of line marker from the input stream.
    /// The end of line marker is defined as either:
    /// - A carriage return (`\r`) followed by a line feed (`\n`).
    /// - A line feed (`\n`) alone.
    /// - A carriage return (`\r`) alone is not valid.
    ///
    /// This function will consume the end of line marker from the input stream.
    /// If the end of line marker is not found, it will return an error.
    pub(crate) fn read_end_of_line_marker(&mut self) -> Result<(), ParserError> {
        if let Some(PdfToken::CarriageReturn) = self.tokenizer.peek() {
            self.tokenizer.read();
        }
        if let Some(PdfToken::NewLine) = self.tokenizer.peek() {
            self.tokenizer.read();
        }
        Ok(())
    }

    pub fn skip_whitespace(&mut self) {
        let _ = self.tokenizer.read_while_u8(Self::is_pdf_whitespace);
    }

    /// Preloads the cross-reference (xref) table for classic (table-based) PDFs without
    /// advancing the parser state.
    ///
    /// This method parses the header to determine the PDF version and, for documents that
    /// use traditional cross-reference tables (PDF 1.x), scans for the final `trailer` at the
    /// end of the file. Using the trailer's `startxref` offset, it seeks to and parses the
    /// xref table, storing it in `self.xref_table`. The tokenizer position is restored to the
    /// point immediately after the header, so subsequent parsing proceeds unaffected.
    ///
    /// Why: Many PDF objects are referenced indirectly. Loading the xref early allows the
    /// parser to resolve indirect references when needed to correctly parse certain objects.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success or a `ParserError` if initialization fails in a fatal way.
    pub fn build_xref_index(&mut self) -> Result<(), ParserError> {
        let version = self.parse_header()?;

        if version.major() != 1 {
            return Ok(());
        }

        // Save the current position (right after header) to restore later.
        let after_header_pos = self.tokenizer.position;

        let bytes = self.tokenizer.input;

        const TRAILER_KEYWORD: &[u8] = b"trailer";

        if let Some(trailer_pos) = bytes
            .windows(TRAILER_KEYWORD.len())
            .rposition(|w| w == TRAILER_KEYWORD)
        {
            self.tokenizer.position = trailer_pos;
            let trailer = self
                .parse_trailer()
                .map_err(|err| ParserError::InitializationError(err.to_string()))?;
            self.tokenizer.position = trailer.offset;

            if let Ok(xref) = self.parse_cross_reference_table() {
                self.xref_table = Some(xref);
            }
        }
        self.tokenizer.position = after_header_pos;

        Ok(())
    }

    /// Resolves an indirect object by its object number using the prebuilt cross-reference table.
    ///
    /// This method temporarily seeks to the byte offset recorded in the xref table,
    /// parses the referenced object, and then restores the tokenizer position so the
    /// parser state is unchanged for the caller.
    ///
    /// Requirements:
    /// - `build_xref_index` must have been called successfully beforehand so that
    ///   `self.xref_table` is populated.
    ///
    /// Notes:
    /// - Generation numbers are currently not considered; the lookup is performed by
    ///   object number only.
    /// - The referenced object is parsed fresh on each call (no caching).
    ///
    /// # Parameters
    ///
    /// - `object_number`: The numeric identifier of the indirect object to resolve.
    ///
    /// # Returns
    ///
    /// The parsed [`ObjectVariant`] corresponding to the given object number.
    ///
    /// # Errors
    /// Returns a [`ParserError`] if:
    /// - No cross-reference table is available (`MissingXrefTable`).
    /// - The xref entry for `object_number` is missing (`MissingXrefEntry`).
    /// - The provided `object_number` cannot be converted to a valid index.
    /// - Parsing the referenced object fails for any reason.
    pub(crate) fn resolve_object_reference(
        &mut self,
        object_number: usize,
    ) -> Result<ObjectVariant, ParserError> {
        let Some(xref) = &self.xref_table else {
            return Err(ParserError::MissingXrefTable);
        };

        let Some(entry) = xref.entries.get(object_number) else {
            return Err(ParserError::MissingXrefEntry { object_number });
        };

        let mark = self.tokenizer.position;
        self.tokenizer.position = entry.byte_offset;
        let object = self.parse_object()?;
        self.tokenizer.position = mark;

        Ok(object)
    }

    /// Reads and parses a number from the PDF input stream.
    ///
    /// This function reads a sequence of ASCII digits from the tokenizer and attempts to parse
    /// them into the specified numeric type. After reading the number, it validates that the
    /// following character is either a valid PDF delimiter or a decimal point.
    ///
    /// # Type Parameters
    ///
    /// - `T`: The target numeric type.
    ///
    /// # Parameters
    ///
    /// - `error`: A convertible error type that will be returned if no digits are found.
    ///
    /// # Returns
    ///
    /// - `Result` indicating success or failure.
    pub(crate) fn read_number<T: FromStr>(
        &mut self,
        skip_whitespace: bool,
    ) -> Result<T, ParserError> {
        let number_str = self.tokenizer.read_while_u8(|b| b.is_ascii_digit());
        if number_str.is_empty() {
            return Err(ParserError::UnexpectedEndOfFile);
        }

        let number = String::from_utf8_lossy(number_str)
            .parse::<T>()
            .or(Err(ParserError::InvalidNumber))?;

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

    /// Reads a keyword literal from the input stream and validates it.
    ///
    /// This function reads a specific keyword literal from the input stream and ensures
    /// that it matches the expected keyword according to the PDF 1.7 specification.
    /// If the literal does not match the expected keyword, an error is returned.
    ///
    /// After successfully reading the keyword, this function also consumes the
    /// end-of-line marker that follows the keyword.
    ///
    /// # Parameters
    ///
    /// - `keyword`: A byte slice representing the expected keyword literal.
    ///
    /// # Returns
    ///
    /// - `Result` indicating success or failure.
    pub(crate) fn read_keyword(&mut self, keyword: &[u8]) -> Result<(), ParserError> {
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

        // Keyword literals are followed by an end-of-line marker.
        self.read_end_of_line_marker()
    }

    fn parse_object_internal(&mut self) -> Result<ObjectVariant, ParserError> {
        self.skip_whitespace();

        let Some(token) = self.tokenizer.peek() else {
            return Err(ParserError::UnexpectedEndOfFile);
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
                    let value = self.parse_trailer();
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
                    ObjectVariant::CrossReferenceTable(self.parse_cross_reference_table()?)
                } else {
                    return Err(ParserError::InvalidToken(char::from(t)));
                }
            }
            PdfToken::DoubleLeftAngleBracket => {
                ObjectVariant::Dictionary(Rc::new(self.parse_dictionary()?))
            }
            PdfToken::LeftAngleBracket => ObjectVariant::HexString(self.parse_hex_string()?),
            PdfToken::Solidus => ObjectVariant::Name(self.parse_name()?),
            PdfToken::Number(_) => {
                // Numbers are ambiguous: could be an indirect object,
                // an indirect reference, or a plain number.
                let mark = self.tokenizer.position;

                // Try parsing as an indirect object first.
                if let Some(o) = self.parse_indirect_object()? {
                    return Ok(o);
                }
                // If that fails, reset and try parsing as a number.
                self.tokenizer.position = mark;
                self.parse_number()?
            }
            PdfToken::Minus => self.parse_number()?,
            PdfToken::Plus => self.parse_number()?,
            PdfToken::Period => self.parse_number()?,
            PdfToken::LeftSquareBracket => ObjectVariant::Array(self.parse_array()?),
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

    pub fn parse_object(&mut self) -> Result<ObjectVariant, ParserError> {
        // Prevent excessive nesting depth.
        if self.current_nesting_depth >= Self::MAX_NESTING_DEPTH {
            return Err(ParserError::NestingDepthExceeded);
        }
        self.current_nesting_depth = self.current_nesting_depth.saturating_add(1);
        let result = self.parse_object_internal();
        self.current_nesting_depth = self.current_nesting_depth.saturating_sub(1);
        result
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_unexpected_token() {
        let input = b"%PDF-1.3
 ";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parser.parse_object();
        assert!(result.is_ok());
    }
}
