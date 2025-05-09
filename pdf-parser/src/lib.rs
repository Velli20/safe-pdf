pub mod array;
pub mod boolean;
pub mod comment;
pub mod cross_reference_table;
pub mod dictionary;
pub mod error;
pub mod header;
pub mod hex_string;
pub mod indirect_object;
pub mod literal_string;
pub mod name;
pub mod null;
pub mod number;
pub mod stream;
pub mod trailer;

use std::{rc::Rc, str::FromStr};

use error::ParserError;
use pdf_object::{
    Value, dictionary::Dictionary, indirect_object::IndirectObjectOrReference, stream::Stream,
    trailer::Trailer,
};
use pdf_tokenizer::{PdfToken, Tokenizer};

pub struct PdfParser<'a> {
    tokenizer: Tokenizer<'a>,
}

impl<'a> From<&'a [u8]> for PdfParser<'a> {
    fn from(input: &'a [u8]) -> Self {
        PdfParser {
            tokenizer: Tokenizer::new(input),
        }
    }
}

/// A trait for parsing PDF objects into a specific type.
///
/// This trait defines a generic interface for parsing PDF objects, allowing
/// implementors to define how a specific type of object is parsed from an input source.
///
/// # Type Parameters
///
/// - `T`: The type of the object that will be produced by the parser.
pub trait ParseObject<T> {
    fn parse(&mut self) -> Result<T, ParserError>;
}

pub trait StreamObjectParser {
    fn parse_stream(&mut self, dictionary: &Dictionary) -> Result<Stream, ParserError>;
}

impl<'a> PdfParser<'a> {
    /// Checks if a character is a whitespace according to PDF 1.7 spec (Section 7.2.2).
    /// Whitespace characters are defined as:
    /// - Null (NUL) - `0x00` (`b'\0'`)
    /// - Horizontal Tab (HT) - `0x09` (`b'\t'`)
    /// - Line Feed (LF) - `0x0A` (`b'\n'`)
    /// - Form Feed (FF) - `0x0C` (`b'\x0C'`)
    /// - Carriage Return (CR) - `0x0D` (`b'\r'`)
    /// - Space (SP) - `0x20` (`b' '`)
    const fn id_pdf_whitespace(c: u8) -> bool {
        matches!(
            c,
            // Whitespace characters (Common ones)
            b' ' | b'\t' | b'\n' | b'\r' | b'\x0C'
        )
    }

    /// Checks if a character is a PDF delimiter according to PDF 1.7 spec (Section 7.2.2).
    /// Whitespace characters (space, tab, newline, etc.) also act as delimiters.
    const fn is_pdf_delimiter(c: u8) -> bool {
        if Self::id_pdf_whitespace(c) {
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
    /// This function will consume the end of line marker from the input stream.
    /// If the end of line marker is not found, it will return an error.
    fn read_end_of_line_marker(&mut self) -> Result<(), ParserError> {
        if let Some(PdfToken::CarriageReturn) = self.tokenizer.peek()? {
            self.tokenizer.read();
        }
        if let Some(PdfToken::NewLine) = self.tokenizer.peek()? {
            self.tokenizer.read();
        }
        Ok(())
    }

    fn skip_whitespace(&mut self) {
        let _ = self.tokenizer.read_while_u8(|b| Self::id_pdf_whitespace(b));
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
    fn read_number<T: FromStr>(&mut self, error: impl Into<ParserError>) -> Result<T, ParserError> {
        let number_str = self.tokenizer.read_while_u8(|b| b.is_ascii_digit());
        if number_str.is_empty() {
            return Err(error.into());
        }

        let number = String::from_utf8_lossy(number_str)
            .parse::<T>()
            .or(Err(ParserError::InvalidNumber))?;

        // Check that the following character after the number is a valid delimiter
        // or a dot (potential decimal number).
        if let Some(d) = self.tokenizer.data().get(0).copied() {
            if !Self::is_pdf_delimiter(d) && d != b'.' {
                return Err(ParserError::MissingDelimiterAfterKeyword(d));
            }
        }

        self.skip_whitespace();

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
    fn read_keyword(&mut self, keyword: &[u8]) -> Result<(), ParserError> {
        let literal = self.tokenizer.read_excactly(keyword.len())?;
        if literal != keyword {
            return Err(ParserError::InvalidKeyword(
                String::from_utf8_lossy(keyword).to_string(),
                String::from_utf8_lossy(literal).to_string(),
            ));
        }

        if let Some(d) = self.tokenizer.data().get(0).copied() {
            if !Self::is_pdf_delimiter(d) {
                return Err(ParserError::MissingDelimiterAfterKeyword(d));
            }
        }

        // Keyword literals are followed by an end-of-line marker.
        self.read_end_of_line_marker()
    }

    pub fn parse_object(&mut self) -> Result<Value, ParserError> {
        if let Some(token) = self.tokenizer.peek()? {
            let value = match token {
                PdfToken::Percent => Value::Comment(self.parse()?),
                PdfToken::DoublePercent => {
                    self.tokenizer.read();
                    const EOF_KEYWORD: &[u8] = b"EOF";

                    // Read the keyword `EOF`.
                    let literal = self.tokenizer.read_excactly(EOF_KEYWORD.len())?;
                    if literal != EOF_KEYWORD {
                        return Err(ParserError::InvalidToken);
                    }
                    return Ok(Value::EndOfFile);
                }
                PdfToken::Alphabetic(t) => {
                    if t == b't' {
                        let start = self.tokenizer.position;
                        let value: Result<Trailer, ParserError> = self.parse();
                        if let Ok(o) = value {
                            return Ok(Value::Trailer(o));
                        }
                        self.tokenizer.position = start;

                        Value::Boolean(self.parse()?)
                    } else if t == b'f' {
                        Value::Boolean(self.parse()?)
                    } else if t == b'n' {
                        Value::Null(self.parse()?)
                    } else if t == b'x' {
                        Value::CrossReferenceTable(self.parse()?)
                    } else {
                        return Err(ParserError::InvalidToken);
                    }
                }
                PdfToken::DoubleLeftAngleBracket => Value::Dictionary(Rc::new(self.parse()?)),
                PdfToken::LeftAngleBracket => Value::HexString(self.parse()?),
                PdfToken::Solidus => Value::Name(self.parse()?),
                PdfToken::Number(_) => {
                    let start = self.tokenizer.position;
                    let value: Result<IndirectObjectOrReference, ParserError> = self.parse();
                    if let Ok(o) = value {
                        return Ok(Value::IndirectObject(Rc::new(o)));
                    }

                    self.tokenizer.position = start;
                    Value::Number(self.parse()?)
                }
                PdfToken::Minus => Value::Number(self.parse()?),
                PdfToken::Plus => Value::Number(self.parse()?),
                PdfToken::LeftSquareBracket => Value::Array(self.parse()?),
                PdfToken::LeftParenthesis => Value::LiteralString(self.parse()?),
                r => {
                    panic!("Unexpected token: {:?}", r);
                }
            };

            return Ok(value);
        }
        Err(ParserError::InvalidToken)
    }
}
