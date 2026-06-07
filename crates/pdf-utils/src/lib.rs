//! Shared low-level utilities for PDF codecs.

pub mod bitreader;
pub mod error;

pub use bitreader::BitReader;
pub use error::BitReaderError;
