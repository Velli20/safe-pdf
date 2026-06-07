//! Pure-Rust CCITT fax and MMR decoding support.

mod ccitt;
mod ccitt_fax_params;
mod ccitt_tables;

pub use ccitt::{CcittDecodeError, decode, decode_rows};
pub use ccitt_fax_params::CCITTFaxParams;
