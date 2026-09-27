use pdf_object_reader::object_error::ObjectError;
use pdf_parser::{error::ParserError, header::HeaderError};
use pdf_resources::error::PdfPagesError;
use pdf_utils::{ErrorTrace, TracedError};
use thiserror::Error;

use crate::decryption::DecryptionError;

/// Errors that can occur while reading a PDF document.
#[derive(Debug, Error)]
pub enum PdfReaderError {
    /// A failure while decoding the document's typed object graph.
    #[error(transparent)]
    ObjectRead(TracedError<pdf_object_reader::ObjectReadError>),
    #[error("missing trailer")]
    MissingTrailer,
    #[error("unexpected reference object at offset {offset}")]
    UnexpectedReference { offset: usize },
    #[error("{0}")]
    ObjectError(#[source] TracedError<ObjectError>),
    #[error("{0}")]
    PdfPagesError(#[source] TracedError<PdfPagesError>),
    #[error("{0}")]
    ParserError(#[source] TracedError<ParserError>),
    #[error("Error parsing PDF header: {0}")]
    HeaderError(#[source] TracedError<HeaderError>),
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
    #[error("failed to resolve {count} object(s); first unresolved at byte offset {first_offset}")]
    UnresolvedObjects { count: usize, first_offset: usize },
}

impl PdfReaderError {
    /// Returns the backtrace captured when a lower-level error entered the reader.
    pub fn trace(&self) -> Option<&ErrorTrace> {
        match self {
            Self::ObjectRead(error) => Some(error.trace()),
            Self::ObjectError(error) => Some(error.trace()),
            Self::PdfPagesError(error) => Some(error.trace()),
            Self::ParserError(error) => Some(error.trace()),
            Self::HeaderError(error) => Some(error.trace()),
            _ => None,
        }
    }

    /// Converts a fatal encryption setup failure into the reader's public error model.
    pub(crate) fn from_decryption_setup(error: DecryptionError) -> Self {
        match error {
            DecryptionError::IncorrectPassword => Self::IncorrectPassword,
            error => Self::DecryptionSetup(error.to_string()),
        }
    }

    pub(crate) fn is_recoverable_optional_object_error(&self) -> bool {
        match self {
            Self::ParserError(_) => true,
            Self::ObjectError(error) => matches!(error.error(), ObjectError::DecompressionError(_)),
            _ => false,
        }
    }

    /// Returns the missing object number when an object failure can be retried later.
    pub(crate) fn unresolved_object_number(&self) -> Option<usize> {
        match self {
            PdfReaderError::ParserError(error) => match error.error() {
                ParserError::ObjectError(ObjectError::FailedResolveObjectReference { obj_num }) => {
                    Some(*obj_num)
                }
                _ => None,
            },
            _ => None,
        }
    }
}

impl From<pdf_object_reader::ObjectReadError> for PdfReaderError {
    #[track_caller]
    fn from(error: pdf_object_reader::ObjectReadError) -> Self {
        Self::ObjectRead(TracedError::new(error))
    }
}

impl From<ObjectError> for PdfReaderError {
    #[track_caller]
    fn from(error: ObjectError) -> Self {
        Self::ObjectError(TracedError::new(error))
    }
}

impl From<PdfPagesError> for PdfReaderError {
    #[track_caller]
    fn from(error: PdfPagesError) -> Self {
        Self::PdfPagesError(TracedError::new(error))
    }
}

impl From<ParserError> for PdfReaderError {
    #[track_caller]
    fn from(error: ParserError) -> Self {
        Self::ParserError(TracedError::new(error))
    }
}

impl From<HeaderError> for PdfReaderError {
    #[track_caller]
    fn from(error: HeaderError) -> Self {
        Self::HeaderError(TracedError::new(error))
    }
}
