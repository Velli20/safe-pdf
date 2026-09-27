//! Shared low-level utilities for PDF codecs.

pub mod bitreader;
pub mod error;
pub mod error_trace;

pub use bitreader::BitReader;
pub use error::BitReaderError;
pub use error_trace::{ErrorTrace, TracedError};
