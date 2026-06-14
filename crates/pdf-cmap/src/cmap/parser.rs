use std::collections::HashMap;

use pdf_object::object_variant::ObjectVariant;
use pdf_parser::{error::ParserError, parser::PdfParser};

use crate::{cmap::token::CMapToken, error::CMapError};

/// Token reader for the small PostScript-like subset used in embedded CMaps.
///
/// This helper sits on top of [`PdfParser`] and exposes only the token kinds
/// needed by font CMap consumers rather than the full PDF object grammar.
pub struct CMapParser<'a> {
    parser: PdfParser<'a>,
    pub(super) to_unicode_map: HashMap<u16, Vec<char>>,
}

impl<'a> From<&'a [u8]> for CMapParser<'a> {
    /// Build a CMap token reader over the provided raw CMap bytes.
    fn from(input: &'a [u8]) -> Self {
        Self {
            parser: PdfParser::from(input),
            to_unicode_map: HashMap::new(),
        }
    }
}

impl<'a> CMapParser<'a> {
    /// Skip whitespace and PostScript/PDF `%...EOL` comments used inside CMaps.
    fn skip_cmap_whitespace_and_comments(&mut self) {
        loop {
            self.parser.skip_whitespace_and_comments();
            if !self.parser.skip_eof_marker_as_comment() {
                break;
            }
        }
    }

    /// Return the next CMap token from the input stream.
    ///
    /// Before reading, this skips PDF whitespace and `%` line comments. Returns
    /// `Ok(None)` after the input is fully consumed.
    pub fn next_token(&mut self) -> Result<Option<CMapToken>, CMapError> {
        self.next_token_with_unknown_operators(false)
    }

    /// Return the next CMap token, preserving unknown operators.
    ///
    /// This is intended for Adobe resource generation, where full
    /// PostScript boilerplate surrounds the CMap sections consumed by the
    /// library parser.
    pub fn next_token_lenient(&mut self) -> Result<Option<CMapToken>, CMapError> {
        self.next_token_with_unknown_operators(true)
    }

    fn next_token_with_unknown_operators(
        &mut self,
        allow_unknown_operators: bool,
    ) -> Result<Option<CMapToken>, CMapError> {
        self.skip_cmap_whitespace_and_comments();

        let Some(byte) = self.parser.tokenizer.data().first().copied() else {
            return Ok(None);
        };

        let token = match byte {
            b'[' => {
                let _ = self.parser.tokenizer.read();
                CMapToken::LeftSquareBracket
            }
            b']' => {
                let _ = self.parser.tokenizer.read();
                CMapToken::RightSquareBracket
            }
            b'(' => CMapToken::LiteralString(
                self.parser
                    .parse_literal_string()
                    .map_err(ParserError::from)?,
            ),
            b'/' => {
                let _ = self.parser.tokenizer.read();
                CMapToken::Name(self.parser.read_operator_name()?.to_vec())
            }
            b'<' => self.parse_left_angle_token()?,
            b'>' => self.parse_right_angle_token(byte)?,
            b'+' | b'-' | b'0'..=b'9' => self.parse_integer_token(byte)?,
            _ if PdfParser::is_pdf_regular_character(byte) => {
                self.parse_keyword_token(allow_unknown_operators)?
            }
            _ => return Err(self.unexpected_token(byte).into()),
        };

        Ok(Some(token))
    }

    fn parse_left_angle_token(&mut self) -> Result<CMapToken, CMapError> {
        if matches!(self.parser.tokenizer.data().get(1).copied(), Some(b'<')) {
            let _ = self.parser.tokenizer.read();
            let _ = self.parser.tokenizer.read();
            Ok(CMapToken::DoubleLeftAngleBracket)
        } else {
            Ok(CMapToken::HexString(self.parser.parse_hex_string()?))
        }
    }

    fn parse_right_angle_token(&mut self, byte: u8) -> Result<CMapToken, CMapError> {
        if matches!(self.parser.tokenizer.data().get(1).copied(), Some(b'>')) {
            let _ = self.parser.tokenizer.read();
            let _ = self.parser.tokenizer.read();
            Ok(CMapToken::DoubleRightAngleBracket)
        } else {
            Err(self.unexpected_token(byte).into())
        }
    }

    fn parse_integer_token(&mut self, byte: u8) -> Result<CMapToken, CMapError> {
        let ObjectVariant::Integer(value) = self.parser.parse_number()? else {
            return Err(self.unexpected_token(byte).into());
        };
        Ok(CMapToken::Integer(value))
    }

    fn parse_keyword_token(
        &mut self,
        allow_unknown_operator: bool,
    ) -> Result<CMapToken, CMapError> {
        let operator = self.parser.read_operator_name()?;

        match operator {
            b"begincmap" => Ok(CMapToken::BeginCMap),
            b"endcmap" => Ok(CMapToken::EndCMap),
            b"begincodespacerange" => Ok(CMapToken::BeginCodeSpaceRange),
            b"endcodespacerange" => Ok(CMapToken::EndCodeSpaceRange),
            b"beginbfchar" => Ok(CMapToken::BeginBfChar),
            b"endbfchar" => Ok(CMapToken::EndBfChar),
            b"beginbfrange" => Ok(CMapToken::BeginBfRange),
            b"endbfrange" => Ok(CMapToken::EndBfRange),
            b"begincidchar" => Ok(CMapToken::BeginCidChar),
            b"endcidchar" => Ok(CMapToken::EndCidChar),
            b"begincidrange" => Ok(CMapToken::BeginCidRange),
            b"endcidrange" => Ok(CMapToken::EndCidRange),
            b"def" => Ok(CMapToken::Def),
            b"usecmap" => Ok(CMapToken::UseCMap),
            _ if allow_unknown_operator => Ok(CMapToken::Operator(operator.to_vec())),
            _ => Err(CMapError::UnknownCMapKeyword(
                String::from_utf8_lossy(operator).into_owned(),
            )),
        }
    }

    fn unexpected_token(&self, byte: u8) -> ParserError {
        ParserError::UnexpectedTokenAt {
            token: String::from_utf8_lossy(&[byte]).into_owned(),
            position: self.parser.tokenizer.position,
        }
    }

    /// Read the next token and require it to be a signed integer.
    ///
    /// # Parameters
    ///
    /// - `message`: Error text used when the next token is absent or is not an
    ///   integer.
    ///
    /// # Returns
    ///
    /// Returns the parsed integer value, or [`CMapError::ParserError`] wrapping
    /// [`ParserError::InvalidNumber`]
    /// carrying `message` when the expected token is missing or has another
    /// kind.
    pub fn expect_integer_token(&mut self, message: &str) -> Result<i64, CMapError> {
        match self.next_token()? {
            Some(CMapToken::Integer(value)) => Ok(value),
            Some(_) | None => Err(ParserError::InvalidNumber(message.to_string()).into()),
        }
    }

    /// Parse and return the ToUnicode mappings from this parser.
    pub fn into_unicode_map(mut self) -> Result<HashMap<u16, Vec<char>>, CMapError> {
        loop {
            let token = self.next_token_lenient()?;

            match token {
                Some(CMapToken::BeginBfChar) => {
                    if !self.parse_bfchar_section()? {
                        return Err(ParserError::UnexpectedEndOfFile.into());
                    }
                }
                Some(CMapToken::BeginBfRange) => {
                    if !self.parse_bfrange_section()? {
                        return Err(ParserError::UnexpectedEndOfFile.into());
                    }
                }
                Some(_) => {}
                None => break,
            }
        }

        Ok(self.to_unicode_map)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{CMapParser, CMapToken};
    use crate::error::CMapError;

    #[test]
    fn skips_comments_and_whitespace() {
        let mut parser = CMapParser::from(b"% comment\r\n  begincmap".as_slice());

        let token = parser.next_token().unwrap();

        assert_eq!(token, Some(CMapToken::BeginCMap));
    }

    #[test]
    fn parses_names_integers_and_brackets() {
        let mut parser = CMapParser::from(b"/WMode -1 [ ]".as_slice());

        assert_eq!(
            parser.next_token().unwrap(),
            Some(CMapToken::Name(b"WMode".to_vec()))
        );
        assert_eq!(parser.next_token().unwrap(), Some(CMapToken::Integer(-1)));
        assert_eq!(
            parser.next_token().unwrap(),
            Some(CMapToken::LeftSquareBracket)
        );
        assert_eq!(
            parser.next_token().unwrap(),
            Some(CMapToken::RightSquareBracket)
        );
        assert_eq!(parser.next_token().unwrap(), None);
    }

    #[test]
    fn parses_hex_strings_with_pdf_rules() {
        let mut parser = CMapParser::from(b"<01 2>".as_slice());

        let token = parser.next_token().unwrap();

        assert_eq!(token, Some(CMapToken::HexString(vec![0x01, 0x20])));
    }

    #[test]
    fn parses_literal_strings_and_dictionary_delimiters() {
        let mut parser = CMapParser::from(b"<< /Registry (G) /Ordering (GrpOne) >>".as_slice());

        assert_eq!(
            parser.next_token().unwrap(),
            Some(CMapToken::DoubleLeftAngleBracket)
        );
        assert_eq!(
            parser.next_token().unwrap(),
            Some(CMapToken::Name(b"Registry".to_vec()))
        );
        assert_eq!(
            parser.next_token().unwrap(),
            Some(CMapToken::LiteralString(b"G".to_vec()))
        );
        assert_eq!(
            parser.next_token().unwrap(),
            Some(CMapToken::Name(b"Ordering".to_vec()))
        );
        assert_eq!(
            parser.next_token().unwrap(),
            Some(CMapToken::LiteralString(b"GrpOne".to_vec()))
        );
        assert_eq!(
            parser.next_token().unwrap(),
            Some(CMapToken::DoubleRightAngleBracket)
        );
        assert_eq!(parser.next_token().unwrap(), None);
    }

    #[test]
    fn treats_double_percent_eof_as_comment_in_embedded_cmaps() {
        let mut parser = CMapParser::from(b"begincmap\n%%EOF\nendcmap".as_slice());

        assert_eq!(parser.next_token().unwrap(), Some(CMapToken::BeginCMap));
        assert_eq!(parser.next_token().unwrap(), Some(CMapToken::EndCMap));
        assert_eq!(parser.next_token().unwrap(), None);
    }

    #[test]
    fn unknown_keyword_returns_cmap_error_with_keyword() {
        let mut parser = CMapParser::from(b"bogus".as_slice());

        let error = parser.next_token().unwrap_err();

        assert_eq!(error, CMapError::UnknownCMapKeyword("bogus".to_string()));
    }

    #[test]
    fn lenient_tokenizer_preserves_unknown_operators() {
        let mut parser = CMapParser::from(b"bogus usecmap".as_slice());

        assert_eq!(
            parser.next_token_lenient().unwrap(),
            Some(CMapToken::Operator(b"bogus".to_vec()))
        );
        assert_eq!(
            parser.next_token_lenient().unwrap(),
            Some(CMapToken::UseCMap)
        );
    }

    #[test]
    fn bfchar_recovery_propagates_token_errors() {
        let mut parser = CMapParser::from(b"<41> bogus > endbfchar".as_slice());

        assert!(parser.parse_bfchar_section().is_err());
    }

    #[test]
    fn bfrange_recovery_propagates_token_errors() {
        let mut parser = CMapParser::from(b"<41> <42> bogus > endbfrange".as_slice());

        assert!(parser.parse_bfrange_section().is_err());
    }
}
