//! Safe Rust JBIG2 decoding support.
//!
//! This is a conservative port of the PDFium/Foxit-derived JBIG2 decoder
//! layout. The implementation keeps the internal bitmap representation
//! 4-byte aligned, but the public output returned to the PDF filter pipeline
//! is tightly packed row data.

mod arith_decoder;
mod compose_op;
mod decode;
pub mod error;
mod fixed_point;
mod generic_refinement_region;
mod generic_region;
mod halftone_region;
mod huffman;
mod image;
mod page;
mod pattern_dictionary;
mod region_info;
mod segment;
mod segment_context;
mod segment_header;
mod stream;
mod symbol_dictionary;
mod text_region;
mod util;

#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub mod bench_support;

pub use decode::decode;
pub use error::Jbig2Error;
