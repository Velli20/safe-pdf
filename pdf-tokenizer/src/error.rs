use crate::PdfToken;

/// An error that can occur while tokenizing a PDF file.
#[derive(Debug, PartialEq)]
pub enum TokenizerError {
    /// This error is used to indicate that the tokenizer has run out of stack space
    SaveStackExchausted,
    UnexpectedToken(Option<PdfToken>, PdfToken),
    InvalidNumber,
    UnexpectedEndOfFile(usize, usize),
}

impl std::fmt::Display for PdfToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfToken::DoublePercent => write!(f, "%%"),
            PdfToken::Percent => write!(f, "%"),
            PdfToken::Plus => write!(f, "+"),
            PdfToken::Period => write!(f, "."),
            PdfToken::Minus => write!(f, "-"),
            PdfToken::DoubleLeftAngleBracket => write!(f, "<<"),
            PdfToken::LeftAngleBracket => write!(f, "<"),
            PdfToken::DoubleRightAngleBracket => write!(f, ">>"),
            PdfToken::RightAngleBracket => write!(f, ">"),
            PdfToken::LeftSquareBracket => write!(f, "["),
            PdfToken::RightSquareBracket => write!(f, "]"),
            PdfToken::LeftParenthesis => write!(f, "("),
            PdfToken::RightParenthesis => write!(f, ")"),
            PdfToken::Solidus => write!(f, "/"),
            PdfToken::Number(num) => write!(f, "{}", num),
            PdfToken::Alphabetic(c) => write!(f, "{}", *c as char),
            PdfToken::NewLine => write!(f, "\n"),
            PdfToken::CarriageReturn => write!(f, "\r"),
            PdfToken::Unknown(byte) => write!(f, "Unknown token: {}", byte),
        }
    }
}

impl std::fmt::Debug for PdfToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(arg0) => f.debug_tuple("Number").field(arg0).finish(),
            Self::Alphabetic(arg0) => write!(f, "Alphabetic({})", *arg0 as char),
            Self::Unknown(arg0) => f.debug_tuple("Unknown").field(arg0).finish(),

            _ => std::fmt::Display::fmt(self, f),
        }
    }
}

impl std::fmt::Display for TokenizerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenizerError::UnexpectedEndOfFile(expected_len, remaining) => write!(
                f,
                "Failed to read exactly {} bytes. Remaining: {}",
                expected_len, remaining
            ),
            TokenizerError::InvalidNumber => write!(f, "Failed to parse number"),
            TokenizerError::SaveStackExchausted => write!(f, "Save stack exhausted"),
            TokenizerError::UnexpectedToken(token, token1) => {
                write!(f, "Unexpected token: {:?}, expected: {:?}", token, token1)
            }
        }
    }
}
