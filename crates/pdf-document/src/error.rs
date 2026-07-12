use pdf_object::error::ObjectError;
use pdf_parser::{error::ParserError, header::HeaderError};
use pdf_resources::error::PdfPagesError;
use thiserror::Error;

use crate::decryption::DecryptionError;

/// Errors that can occur while reading a PDF document.
#[derive(Debug, Error)]
pub enum PdfReaderError {
    #[error("missing trailer")]
    MissingTrailer,
    #[error("unexpected reference object at offset {offset}")]
    UnexpectedReference { offset: usize },
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("{0}")]
    PdfPagesError(#[from] PdfPagesError),
    #[error("{0}")]
    ParserError(#[from] ParserError),
    #[error("Error parsing PDF header: {0}")]
    HeaderError(#[from] HeaderError),
    #[error("unsupported PDF version: {0}.{1}")]
    UnsupportedVersion(u8, u8),
    #[error("invalid cross-reference table at offset {offset}")]
    InvalidXrefAtOffset { offset: usize },
    #[error("unsupported encryption version: {version}")]
    UnsupportedEncryptionVersion { version: i32 },
    #[error("incorrect password")]
    IncorrectPassword,
    #[error("unable to initialize document decryption: {0}")]
    DecryptionSetup(String),
    #[error("missing document ID required for encryption")]
    MissingDocumentId,
    #[error(
        "failed to resolve {count} object(s) after {iterations} iteration(s); \
         first unresolved at byte offset {first_offset}"
    )]
    UnresolvedObjects {
        count: usize,
        iterations: usize,
        first_offset: usize,
    },
}

impl PdfReaderError {
    /// Converts a fatal encryption setup failure into the reader's public error model.
    pub(crate) fn from_decryption_setup(error: DecryptionError) -> Self {
        match error {
            DecryptionError::IncorrectPassword => Self::IncorrectPassword,
            error => Self::DecryptionSetup(error.to_string()),
        }
    }
}
