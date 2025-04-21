use std::vec;

use error::TokenizerError;

pub mod error;

struct State<'a> {
    position: usize,
    input: &'a [u8],
}

pub struct Tokenizer<'a> {
    input: &'a [u8],
    position: usize,
    state_stack: Vec<State<'a>>,
}

#[derive(Debug, PartialEq)]
pub enum Token {
    Literal(String),

    Percent,
    LessThan,
    GreaterThan,
    Number(i32),
    NewLine,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Tokenizer {
            input,
            position: 0,
            state_stack: vec![State { position: 0, input }],
        }
    }

    fn current_state(&self) -> Result<&State<'a>, TokenizerError> {
        if let Some(state) = self.state_stack.last() {
            return Ok(state);
        }
        Err(TokenizerError::SaveStackExchausted)
    }
    fn current_state_mut(&mut self) -> Result<&mut State<'a>, TokenizerError> {
        if let Some(state) = self.state_stack.last_mut() {
            return Ok(state);
        }
        Err(TokenizerError::SaveStackExchausted)
    }

    /// Pushes a new state onto the stack.
    /// This is used to save the current position in the input.
    /// The new state is initialized with the current position and input.
    pub fn save_state(&mut self) -> Result<(), TokenizerError> {
        let state = State {
            position: self.position,
            input: self.input,
        };
        self.state_stack.push(state);
        Ok(())
    }

    pub fn restore_state(&mut self) -> Result<(), TokenizerError> {
        if let Some(state) = self.state_stack.pop() {
            self.position = state.position;
            self.input = state.input;
            Ok(())
        } else {
            Err(TokenizerError::SaveStackExchausted)
        }
    }

    pub fn expect(&mut self, expected: Token) -> Result<(), TokenizerError> {
        match self.read() {
            Some(token) if token == expected => Ok(()),
            Some(token) => Err(TokenizerError::UnexpectedToken(Some(token), expected)),
            None => Err(TokenizerError::UnexpectedToken(None, expected)),
        }
    }

    pub fn peek(&mut self) -> Option<u8> {
        self.skip_whitespace();
        if self.position + 1 < self.input.len() {
            Some(self.input[self.position + 1])
        } else {
            None
        }
    }

    /// Reads the next token from the input.
    pub fn read(&mut self) -> Option<Token> {
        self.skip_whitespace();
        if self.position >= self.input.len() {
            return None;
        }
        let byte = self.input[self.position];

        match byte {
            b'%' => {
                self.position += 1;
                Some(Token::Percent)
            }
            b'<' => {
                self.position += 1;
                Some(Token::LessThan)
            }
            b'>' => {
                self.position += 1;
                Some(Token::GreaterThan)
            }
            b'0'..=b'9' => {
                let start = self.position;
                while self.position < self.input.len() && self.input[self.position].is_ascii_digit()
                {
                    self.position += 1;
                }
                let number_str = &self.input[start..self.position];
                let number = String::from_utf8_lossy(number_str)
                    .parse::<i32>()
                    .ok()
                    .map(Token::Number);
                number
            }
            b'\n' => {
                self.position += 1;
                Some(Token::NewLine)
            }
            _ if byte.is_ascii_alphanumeric() => {
                let literal = self.read_while(|b| b != b'\n');
                Some(Token::Literal(literal))
            }
            _ => None,
        }
    }

    fn skip_whitespace(&mut self) {
        while self.position < self.input.len() && self.input[self.position].is_ascii_whitespace() {
            self.position += 1;
        }
    }

    fn read_while<F>(&mut self, condition: F) -> String
    where
        F: Fn(u8) -> bool,
    {
        let start = self.position;
        while self.position < self.input.len() && condition(self.input[self.position]) {
            self.position += 1;
        }
        String::from_utf8_lossy(&self.input[start..self.position]).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer() {
        let input = b"%PDF-1.4\n% This is a comment\n";
        let mut tokenizer = Tokenizer::new(input);
        assert_eq!(tokenizer.read(), Some(Token::Percent));
        assert_eq!(
            tokenizer.read(),
            Some(Token::Literal("PDF-1.4".to_string()))
        );
        assert_eq!(tokenizer.read(), Some(Token::Percent));
        assert_eq!(
            tokenizer.read(),
            Some(Token::Literal("This is a comment".to_string()))
        );
        assert_eq!(tokenizer.read(), None);
    }

    #[test]
    fn test_save_restore_state() {
        let input = b"%PDF-1.4\n% This is a comment\n";
        let mut tokenizer = Tokenizer::new(input);
        assert_eq!(tokenizer.read(), Some(Token::Percent));
        tokenizer.save_state().unwrap();
        assert_eq!(
            tokenizer.read(),
            Some(Token::Literal("PDF-1.4".to_string()))
        );
        tokenizer.restore_state().unwrap();
        assert_eq!(tokenizer.read(), Some(Token::Percent));
    }
    #[test]
    fn test_tokenize_number() {
        let input = b"123 456 789";
        let mut tokenizer = Tokenizer::new(input);
        assert_eq!(tokenizer.read(), Some(Token::Number(123)));
        assert_eq!(tokenizer.read(), Some(Token::Number(456)));
        assert_eq!(tokenizer.read(), Some(Token::Number(789)));
        assert_eq!(tokenizer.read(), None);
    }
}
