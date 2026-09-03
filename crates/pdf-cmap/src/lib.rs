//! PDF CMap parsing and predefined CMap lookup.

pub mod cmap;
mod cmap_support;
pub mod error;
pub mod mapping;
pub mod predefined;
pub mod to_unicode;
pub mod type0;
pub mod unicode_sequence;
pub mod writing_mode;

pub use mapping::{Cid, CidMapping, PdfCMap, PdfCode, ToUnicodeMap};
pub use to_unicode::{IdentityToUnicodeMap, ToUnicodeCMap};
pub use type0::Type0EncodingCMap;
pub use unicode_sequence::UnicodeSequence;
pub use writing_mode::{WritingMode, WritingModeNameError};
