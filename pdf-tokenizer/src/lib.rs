pub mod error;

use error::TokenizerError;

pub struct Tokenizer<'a> {
    input: &'a [u8],
    pub position: usize,
}

/// Represents the possible tokens in a PDF file.
#[derive(PartialEq)]
pub enum PdfToken {
    /// Represents the '%' token.
    Percent,
    /// Represents the '%%' token.
    DoublePercent,
    /// Plus sign '+'.
    Plus,
    /// Period '.'.
    Period,
    /// Minus sign '-'.
    Minus,
    /// Represents the '<<' token.
    DoubleLeftAngleBracket,
    /// Represents the '<' token.
    LeftAngleBracket,
    /// Represents the '>>' token.
    DoubleRightAngleBracket,
    /// Represents the '>' token.
    RightAngleBracket,
    /// Represents the '[' token.
    LeftSquareBracket,
    /// Represents the ']' token.
    RightSquareBracket,
    /// Represents the '(' token.
    LeftParenthesis,
    /// Represents the ')' token.
    RightParenthesis,
    /// Represents the '/' token.
    Solidus,
    /// A number '0..9'.
    Number(i32),
    /// An alphabetic character 'A..Z', 'a..z'.
    Alphabetic(u8),
    /// A newline '\n' character.
    NewLine,
    /// A newline '\r' character.
    CarriageReturn,
    /// An unknown token.
    Unknown(u8),
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Tokenizer { input, position: 0 }
    }

    pub fn expect(&mut self, expected: PdfToken) -> Result<(), TokenizerError> {
        match self.read() {
            Some(token) if token == expected => Ok(()),
            Some(token) => Err(TokenizerError::UnexpectedToken(Some(token), expected)),
            None => Err(TokenizerError::UnexpectedToken(None, expected)),
        }
    }

    pub fn peek(&mut self) -> Result<Option<PdfToken>, TokenizerError> {
        let state = self.position;
        let token = self.read();
        self.position = state;
        Ok(token)
    }

    /// Reads the next token from the input.
    pub fn read(&mut self) -> Option<PdfToken> {
        if self.position >= self.input.len() {
            return None;
        }
        let byte = self.input[self.position];

        match byte {
            b'\n' => {
                self.position += 1;
                Some(PdfToken::NewLine)
            }
            b'\r' => {
                self.position += 1;
                Some(PdfToken::CarriageReturn)
            }
            b'%' => {
                self.position += 1;
                if let Some(b'%') = self.input.get(self.position) {
                    self.position += 1;
                    return Some(PdfToken::DoublePercent);
                }
                Some(PdfToken::Percent)
            }
            b'-' => {
                self.position += 1;
                Some(PdfToken::Minus)
            }
            b'+' => {
                self.position += 1;
                Some(PdfToken::Plus)
            }
            b'.' => {
                self.position += 1;
                Some(PdfToken::Period)
            }
            b'/' => {
                self.position += 1;
                Some(PdfToken::Solidus)
            }
            b'<' => {
                self.position += 1;
                if let Some(b'<') = self.input.get(self.position) {
                    self.position += 1;
                    return Some(PdfToken::DoubleLeftAngleBracket);
                }
                Some(PdfToken::LeftAngleBracket)
            }
            b'>' => {
                self.position += 1;
                if let Some(b'>') = self.input.get(self.position) {
                    self.position += 1;
                    return Some(PdfToken::DoubleRightAngleBracket);
                }
                Some(PdfToken::RightAngleBracket)
            }
            b'[' => {
                self.position += 1;
                Some(PdfToken::LeftSquareBracket)
            }
            b']' => {
                self.position += 1;
                Some(PdfToken::RightSquareBracket)
            }
            b'(' => {
                self.position += 1;
                Some(PdfToken::LeftParenthesis)
            }
            b')' => {
                self.position += 1;
                Some(PdfToken::RightParenthesis)
            }
            b'0'..=b'9' => {
                self.position += 1;
                Some(PdfToken::Number((byte - b'0') as i32))
            }
            b'A'..=b'Z' | b'a'..=b'z' => {
                self.position += 1;
                Some(PdfToken::Alphabetic(byte))
            }

            _ => None,
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.input[self.position..]
    }

    pub fn read_excactly(&mut self, length: usize) -> Result<&[u8], TokenizerError> {
        if self.position + length > self.input.len() {
            return Err(TokenizerError::UnexpectedEndOfFile(
                length,
                self.input.len() - self.position,
            ));
        }
        let result = &self.input[self.position..self.position + length];
        self.position += length;
        Ok(result)
    }

    pub fn read_while_u8<F>(&mut self, condition: F) -> &'a [u8]
    where
        F: Fn(u8) -> bool,
    {
        let start = self.position;
        while self.position < self.input.len() && condition(self.input[self.position]) {
            self.position += 1;
        }
        &self.input[start..self.position]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_number() {
        let input = b"123";
        let mut tokenizer = Tokenizer::new(input);
        assert_eq!(tokenizer.read(), Some(PdfToken::Number(1)));
        assert_eq!(tokenizer.read(), Some(PdfToken::Number(2)));
        assert_eq!(tokenizer.read(), Some(PdfToken::Number(3)));
        assert_eq!(tokenizer.read(), None);
    }
}
