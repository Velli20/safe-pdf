pub mod error;

use std::str::FromStr;

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

    pub fn read_number<T: FromStr>(&mut self) -> Result<T, TokenizerError> {
        let number_str = self.read_while_u8(|b| b.is_ascii_digit());
        if number_str.is_empty() {
            return Err(TokenizerError::InvalidNumber);
        }

        let number = String::from_utf8_lossy(number_str)
            .parse::<T>()
            .or(Err(TokenizerError::InvalidNumber))?;

        Ok(number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Debug;

    #[test]
    fn test_tokenize_number() {
        let input = b"123";
        let mut tokenizer = Tokenizer::new(input);
        assert_eq!(tokenizer.read(), Some(PdfToken::Number(1)));
        assert_eq!(tokenizer.read(), Some(PdfToken::Number(2)));
        assert_eq!(tokenizer.read(), Some(PdfToken::Number(3)));
        assert_eq!(tokenizer.read(), None);
    }

    // Helper function to comprehensively test read_number
    fn check_read_number<T: FromStr + PartialEq + Debug>(
        input: &'static [u8],
        expected_output: Result<T, TokenizerError>,
        expected_pos_after_read: usize,
        expected_remaining_data: &'static [u8],
    ) {
        let mut tokenizer = Tokenizer::new(input);
        let result: Result<T, TokenizerError> = tokenizer.read_number();

        assert_eq!(
            result,
            expected_output,
            "Test failed for input: \"{}\". Expected result: {:?}, got: {:?}.",
            String::from_utf8_lossy(input),
            expected_output,
            result
        );

        assert_eq!(
            tokenizer.position,
            expected_pos_after_read,
            "Test failed for input: \"{}\". Expected position: {}, got: {}.",
            String::from_utf8_lossy(input),
            expected_pos_after_read,
            tokenizer.position
        );

        assert_eq!(
            tokenizer.data(),
            expected_remaining_data,
            "Test failed for input: \"{}\". Expected remaining data: \"{}\", got: \"{}\".",
            String::from_utf8_lossy(input),
            String::from_utf8_lossy(expected_remaining_data),
            String::from_utf8_lossy(tokenizer.data())
        );
    }

    #[test]
    fn test_read_number_generic_cases() {
        // Successful parsing
        check_read_number::<u32>(b"123 abc", Ok(123), 3, b" abc");
        check_read_number::<u64>(b"9876543210", Ok(9876543210), 10, b"");
        check_read_number::<u8>(b"0xyz", Ok(0), 1, b"xyz");
        check_read_number::<String>(b"007 bond", Ok("007".to_string()), 3, b" bond");
        check_read_number::<u16>(b"65535stop", Ok(65535), 5, b"stop");
        check_read_number::<i32>(b"123.45", Ok(123), 3, b".45"); // Reads only the integer part

        // Input with no digits at the current position
        check_read_number::<u32>(b"abc123", Err(TokenizerError::InvalidNumber), 0, b"abc123");
        check_read_number::<u32>(b" xyz", Err(TokenizerError::InvalidNumber), 0, b" xyz");
        check_read_number::<u32>(b"", Err(TokenizerError::InvalidNumber), 0, b"");
        check_read_number::<String>(
            b" no_digits",
            Err(TokenizerError::InvalidNumber),
            0,
            b" no_digits",
        );

        // For u8 (max 255)
        check_read_number::<u8>(b"256", Err(TokenizerError::InvalidNumber), 3, b"");
        check_read_number::<u8>(
            b"300 and more",
            Err(TokenizerError::InvalidNumber),
            3,
            b" and more",
        );

        // For i16 (max 32767)
        check_read_number::<i16>(b"32768", Err(TokenizerError::InvalidNumber), 5, b"");
        check_read_number::<i16>(
            b"100000rest",
            Err(TokenizerError::InvalidNumber),
            6,
            b"rest",
        );

        // For i8 (min -128, max 127)
        // While `read_number` itself doesn't handle signs, if `T` was signed and `FromStr` for `T`
        // expected only non-negative numbers from this particular path, this would test it.
        // However, `read_number` only extracts positive digit sequences.
        // The following tests `FromStr` for `i8` with a number that's too large positive.
        check_read_number::<i8>(b"128", Err(TokenizerError::InvalidNumber), 3, b"");
    }
}
