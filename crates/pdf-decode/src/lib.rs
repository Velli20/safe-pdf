//! Decode PDF sample data, decode maps, and indexed color lookup tables.
//!
//! This crate provides small utilities that sit between parsed PDF objects and
//! higher-level image or function decoding code. It handles sample unpacking,
//! `/Decode` range handling, and palette expansion.

mod error;
mod indexed;
mod layout;
mod map;
mod range;
mod samples;

pub use error::DecodeError;
pub use indexed::expand_indexed_values;
pub use layout::SampleLayout;
pub use map::DecodeMap;
pub use range::DecodeRange;
pub use samples::{decode_normalized_samples, decode_sample_bytes, decode_sample_codes};
