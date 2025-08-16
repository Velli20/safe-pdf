use crate::PdfToken;
use thiserror::Error;

/// An error that can occur while tokenizing a PDF file.
#[derive(Debug, PartialEq, Error)]
pub enum TokenizerError {
    /// Occurs when a specific token was expected, but a different token
    /// (or end of input) was encountered.
    /// The first `Option<PdfToken>` is the actual token found (None if end of input),
    /// and the second `PdfToken` is the token that was expected.
    #[error("Unexpected token: {0:?}, expected: {1:?}")]
    UnexpectedToken(Option<PdfToken>, PdfToken),
    /// Raised when the end of the input stream was reached prematurely
    /// while trying to read a specific number of bytes.
    /// The first `usize` is the expected number of bytes, and the second `usize`
    /// is the number of bytes actually remaining or read.
    #[error(
        "Unexpected end of file: expected to read {0} bytes, but only {1} bytes were available"
    )]
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
            PdfToken::NewLine => writeln!(f),
            PdfToken::CarriageReturn => write!(f, "\r"),
            PdfToken::Unknown(byte) => write!(f, "Unknown token: {}", byte),
            PdfToken::Space => write!(f, " "),
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
