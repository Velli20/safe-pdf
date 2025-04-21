use error::ParserError;
use pdf_tokenizer::{Token, Tokenizer};

pub struct PdfParser<'a> {
    tokenizer: Tokenizer<'a>,
}

pub mod error;

pub struct Version {
    major: u8,
    minor: u8,
}

pub enum Value {
    IndirectValue {
        object_number: i32,
        generation_number: i32,
    },
    Stream {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    Dictionary {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    Array {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    String {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    Name {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    Number {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    Boolean {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    Null {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    HexString {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    Literal {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    Reference {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    IndirectReference {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
    IndirectStream {
        object_number: u32,
        generation_number: u16,
        length: usize,
    },
}

impl<'a> PdfParser<'a> {
    fn find_header_start(input: &'a [u8]) -> Result<&'a [u8], ParserError> {
        // PDF 1.7 spec, APPENDIX H, 3.4.1 "File Header":
        // "13. Acrobat viewers require only that the header appear somewhere within the first 1024 bytes of the file."
        // ...which of course means files depend on it.
        // All offsets in the file are relative to the header start, not to the start of the file.

        const HEADER_START_SEQUENCE: &[u8] = b"%PDF-";

        let result = input
            .windows(5)
            .position(|window| window == HEADER_START_SEQUENCE);
        if let Some(offset) = result {
            return Ok(&input[offset..]);
        } else {
            return Err(ParserError::InvalidHeader);
        }
    }

    pub fn from(input: &'a [u8]) -> Result<PdfParser<'a>, ParserError> {
        let input = Self::find_header_start(input)?;
        Ok(PdfParser {
            tokenizer: Tokenizer::new(input),
        })
    }

    pub fn parse_header(&mut self) -> Result<Version, ParserError> {
        // PDF 1.7 spec, APPENDIX H, 3.4.1 "File Header":
        // "The header consists of the characters %PDF- followed by a version number."
        // The version number is a sequence of digits and periods.
        // The header must be followed by a newline character (ASCII 10).
        // The header may be preceded by whitespace or comments.
        // The header may be followed by whitespace or comments.
        // The header may be followed by any number of bytes.
        // The header may be followed by any number of bytes.

        self.tokenizer.expect(Token::Percent).unwrap();

        let token = self
            .tokenizer
            .read()
            .ok_or(ParserError::UnexpectedEndOfFile)?;

        match token {
            Token::Literal(literal) => {
                if !literal.starts_with("PDF-") {
                    return Err(ParserError::InvalidHeader);
                }

                let literal = &literal["PDF-".len()..];
                let parts = literal.split('.').collect::<Vec<_>>();

                if parts.len() != 2 {
                    return Err(ParserError::InvalidHeader);
                }
                let major = parts[0]
                    .parse::<u8>()
                    .map_err(|_| ParserError::InvalidHeader)?;
                let minor = parts[1]
                    .parse::<u8>()
                    .map_err(|_| ParserError::InvalidHeader)?;

                return Ok(Version { major, minor });
            }
            Token::Percent => todo!(),
            Token::LessThan => todo!(),
            Token::GreaterThan => todo!(),
            Token::Number(_) => todo!(),
            Token::NewLine => todo!(),
        }
    }

    pub fn parse_indirect_value(&mut self) -> Result<Option<Value>, ParserError> {
        self.tokenizer.save_state()?;
        if let Some(Token::Number(object_number)) = self.tokenizer.read() {
            if let Some(Token::Number(generation_number)) = self.tokenizer.read() {
                if let Some(Token::Literal(literal)) = self.tokenizer.read() {
                    if literal == "obj" {
                        return Ok(Some(Value::IndirectValue {
                            object_number,
                            generation_number,
                        }));
                    }
                }
            }
        }
        self.tokenizer.restore_state()?;
        Ok(None)
    }

    pub fn parse_trailer(&mut self) -> Result<(), ParserError> {
        // PDF 1.7 spec, APPENDIX H, 3.4.2 "File Trailer":
        // "The trailer consists of the characters %%EOF followed by a version number."
        // The version number is a sequence of digits and periods.
        // The trailer must be followed by a newline character (ASCII 10).
        // The trailer may be preceded by whitespace or comments.
        // The trailer may be followed by whitespace or comments.
        // The trailer may be followed by any number of bytes.
        // The trailer may be followed by any number of bytes.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_header_start_valid() {
        let input = b"Some random data %PDF-1.7 more data";
        let result = PdfParser::find_header_start(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"%PDF-1.7 more data");
    }

    #[test]
    fn test_indirect_value_valid() {
        let input = b"%PDF-1.7\n0 0 obj\n";
        let mut parser = PdfParser::from(input).unwrap();
        let _version = parser.parse_header().unwrap();

        let result = parser.parse_indirect_value().unwrap();
        assert!(result.is_some());
        if let Some(Value::IndirectValue {
            object_number,
            generation_number,
        }) = result
        {
            assert_eq!(object_number, 0);
            assert_eq!(generation_number, 0);
        } else {
            panic!("Expected IndirectValue");
        }
    }

    #[test]
    fn test_find_header_start_invalid() {
        let input = b"Some random data without header";
        let result = PdfParser::find_header_start(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_header_valid() {
        let input = b"%PDF-1.7\n";
        let mut parser = PdfParser::from(input).unwrap();
        let version = parser.parse_header().unwrap();
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 7);
    }

    #[test]
    fn test_parse_header_invalid_format() {
        let input = b"%PDF-1.x";
        let mut parser = PdfParser::from(input).unwrap();
        let result = parser.parse_header();
        assert!(result.is_err());
    }

    // #[test]
    // fn test_parse_header_unexpected_token() {
    //     let input = b"Some random data";
    //     let mut parser = PdfParser::from(input).unwrap();
    //     let result = parser.parse_header();
    //     assert!(result.is_err());
    // }

    #[test]
    fn test_from_valid_input() {
        let input = b"Some data %PDF-1.7 more data";
        let parser = PdfParser::from(input);
        assert!(parser.is_ok());
    }

    #[test]
    fn test_from_invalid_input() {
        let input = b"Some data without header";
        let parser = PdfParser::from(input);
        assert!(parser.is_err());
    }
}
