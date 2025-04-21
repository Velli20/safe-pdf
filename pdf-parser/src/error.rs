use pdf_tokenizer::error::TokenizerError;

#[derive(Debug)]
pub enum ParserError {
    InvalidHeader,
    InvalidTrailer,
    InvalidToken,
    UnexpectedEndOfFile,
    TokenizerError(TokenizerError),
}

impl From<TokenizerError> for ParserError {
    fn from(tokenizer_error: TokenizerError) -> Self {
        ParserError::TokenizerError(tokenizer_error)
    }
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParserError::InvalidHeader => write!(f, "Invalid PDF header"),
            ParserError::InvalidTrailer => write!(f, "Invalid PDF trailer"),
            ParserError::InvalidToken => write!(f, "Invalid token"),
            ParserError::UnexpectedEndOfFile => write!(f, "Unexpected end of file"),
            ParserError::TokenizerError(tokenizer_error) => tokenizer_error.fmt(f),
        }
    }
}
