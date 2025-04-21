use crate::Token;

/// An error that can occur while tokenizing a PDF file.
#[derive(Debug)]
pub enum TokenizerError {
    /// This error is used to indicate that the tokenizer has run out of stack space
    SaveStackExchausted,
    UnexpectedToken(Option<Token>, Token),
}

impl std::fmt::Display for TokenizerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenizerError::SaveStackExchausted => write!(f, "Save stack exhausted"),
            TokenizerError::UnexpectedToken(token, token1) => {
                write!(f, "Unexpected token: {:?}, expected: {:?}", token, token1)
            }
        }
    }
}
