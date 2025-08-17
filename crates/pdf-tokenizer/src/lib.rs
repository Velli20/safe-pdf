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
    /// Space ' '.
    Space,
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

    /// Safely advance the internal cursor by `n` bytes.
    /// Returns `true` if the advance succeeded, `false` if it would move past the end.
    #[inline]
    fn advance(&mut self, n: usize) -> bool {
        if n == 0 {
            return true;
        }
        match self.position.checked_add(n) {
            Some(new_pos) if new_pos <= self.input.len() => {
                self.position = new_pos;
                true
            }
            _ => false,
        }
    }

    /// Peek the current byte without consuming it.
    #[inline]
    fn peek_byte(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }

    /// Consume and return the next byte.
    #[inline]
    fn next_byte(&mut self) -> Option<u8> {
        let b = self.peek_byte()?;
        let _ = self.advance(1); // safe advance after peek
        Some(b)
    }

    /// If the next byte matches `expected`, consume it and return true.
    #[inline]
    fn match_next(&mut self, expected: u8) -> bool {
        matches!(self.peek_byte(), Some(b) if b == expected) && {
            // safe to advance by 1; advance() returns false only if out-of-bounds
            self.advance(1)
        }
    }

    pub fn expect(&mut self, expected: PdfToken) -> Result<(), TokenizerError> {
        match self.read() {
            Some(token) if token == expected => Ok(()),
            Some(token) => Err(TokenizerError::UnexpectedToken(Some(token), expected)),
            None => Err(TokenizerError::UnexpectedToken(None, expected)),
        }
    }

    pub fn peek(&mut self) -> Option<PdfToken> {
        let state = self.position;
        let token = self.read();
        self.position = state;
        token
    }

    /// Reads the next token from the input.
    pub fn read(&mut self) -> Option<PdfToken> {
        let byte = self.next_byte()?;
        use PdfToken::*;
        Some(match byte {
            b'\n' => NewLine,
            b'\r' => CarriageReturn,
            b' ' => Space,
            b'%' => {
                if self.match_next(b'%') {
                    DoublePercent
                } else {
                    Percent
                }
            }
            b'-' => Minus,
            b'+' => Plus,
            b'.' => Period,
            b'/' => Solidus,
            b'<' => {
                if self.match_next(b'<') {
                    DoubleLeftAngleBracket
                } else {
                    LeftAngleBracket
                }
            }
            b'>' => {
                if self.match_next(b'>') {
                    DoubleRightAngleBracket
                } else {
                    RightAngleBracket
                }
            }
            b'[' => LeftSquareBracket,
            b']' => RightSquareBracket,
            b'(' => LeftParenthesis,
            b')' => RightParenthesis,
            b'0'..=b'9' => {
                // Safe because of the pattern match restricting range.
                // Using checked_sub then unwrap_or is fine because match arm ensures byte >= b'0'.
                let digit = byte.saturating_sub(b'0');
                Number(i32::from(digit))
            }
            b'A'..=b'Z' | b'a'..=b'z' => Alphabetic(byte),
            _ => return None,
        })
    }

    pub fn data(&self) -> &[u8] {
        &self.input[self.position..]
    }

    pub fn read_excactly(&mut self, length: usize) -> Result<&[u8], TokenizerError> {
        let available = self.input.len().saturating_sub(self.position);
        if length > available {
            return Err(TokenizerError::UnexpectedEndOfFile(length, available));
        }
        // Safe: validated above length <= available.
        // checked_add cannot fail due to previous bounds validation.
        let end = match self.position.checked_add(length) {
            Some(e) => e,
            None => return Ok(&[]),
        };
        let slice = &self.input[self.position..end];
        self.position = end;
        Ok(slice)
    }

    pub fn read_while_u8<F>(&mut self, condition: F) -> &'a [u8]
    where
        F: Fn(u8) -> bool,
    {
        let start = self.position;
        while let Some(&b) = self.input.get(self.position) {
            if condition(b) {
                let _ = self.advance(1);
            } else {
                break;
            }
        }
        &self.input[start..self.position]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
