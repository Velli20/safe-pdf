use crate::{error::ParserError, parser::PdfParser};

/// Token kinds used by embedded font CMap streams.
#[derive(Debug, PartialEq, Eq)]
pub enum CMapToken {
    /// A regular PostScript-like operator or keyword.
    Operator(Vec<u8>),
    /// A name object without its leading `/`.
    Name(Vec<u8>),
    /// An integer literal.
    Integer(i64),
    /// A PDF hex string decoded into raw bytes.
    HexString(Vec<u8>),
    /// `[` token.
    LeftSquareBracket,
    /// `]` token.
    RightSquareBracket,
}

/// Token reader for the small PostScript-like subset used in embedded CMaps.
///
/// This helper sits on top of [`PdfParser`] and exposes only the token kinds
/// needed by font CMap consumers rather than the full PDF object grammar.
pub struct CMapParser<'a> {
    parser: PdfParser<'a>,
}

impl<'a> From<&'a [u8]> for CMapParser<'a> {
    /// Build a CMap token reader over the provided raw CMap bytes.
    fn from(input: &'a [u8]) -> Self {
        Self {
            parser: PdfParser::from(input),
        }
    }
}

impl<'a> CMapParser<'a> {
    /// Return the next CMap token from the input stream.
    ///
    /// Before reading, this skips PDF whitespace and `%` line comments. Returns
    /// `Ok(None)` after the input is fully consumed.
    pub fn next_token(&mut self) -> Result<Option<CMapToken>, ParserError> {
        self.parser.skip_whitespace_and_comments();

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
            b'/' => {
                let _ = self.parser.tokenizer.read();
                CMapToken::Name(self.parser.read_operator_name()?.to_vec())
            }
            b'<' => CMapToken::HexString(self.parser.parse_hex_string()?),
            b'+' | b'-' | b'0'..=b'9' => CMapToken::Integer(self.parse_integer()?),
            _ if PdfParser::is_pdf_regular_character(byte) => {
                CMapToken::Operator(self.parser.read_operator_name()?.to_vec())
            }
            _ => {
                return Err(ParserError::UnexpectedTokenAt {
                    token: String::from_utf8_lossy(&[byte]).into_owned(),
                    position: self.parser.tokenizer.position,
                });
            }
        };

        Ok(Some(token))
    }

    /// Parse a signed integer token used by embedded CMap syntax.
    ///
    /// This accepts an optional leading `+` or `-` and then delegates digit
    /// parsing to [`PdfParser::read_number`].
    fn parse_integer(&mut self) -> Result<i64, ParserError> {
        let mut is_negative = false;

        match self.parser.tokenizer.data().first().copied() {
            Some(b'+') => {
                let _ = self.parser.tokenizer.read();
            }
            Some(b'-') => {
                let _ = self.parser.tokenizer.read();
                is_negative = true;
            }
            _ => {}
        }

        let value = self.parser.read_number::<i64>(false)?;
        if is_negative {
            value.checked_neg().ok_or_else(|| {
                ParserError::InvalidNumber("integer underflow while parsing CMap token".to_string())
            })
        } else {
            Ok(value)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{CMapParser, CMapToken};

    #[test]
    fn skips_comments_and_whitespace() {
        let mut parser = CMapParser::from(b"% comment\r\n  begincmap".as_slice());

        let token = parser.next_token().unwrap();

        assert_eq!(token, Some(CMapToken::Operator(b"begincmap".to_vec())));
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
}
