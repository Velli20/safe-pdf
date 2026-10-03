//! Caller-controlled bounds for decoding image data from an untrusted PDF.
//!
//! The SIZ marker declares dimensions, components, and tiles. They are checked
//! against these limits with checked arithmetic before any tile workspace is
//! allocated.

use crate::container::InputFormat;

/// Upper bounds for a single JPEG 2000 image.
///
/// The caller must choose values appropriate to its PDF rendering budget.
/// There is deliberately no unbounded default. Input bytes are borrowed;
/// `max_working_bytes` bounds only decoder-owned temporary storage, not memory
/// retained by a [`crate::TileSink`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecoderLimits {
    /// Maximum length of the supplied codestream or JP2 byte slice.
    pub max_input_bytes: usize,
    /// Maximum width multiplied by height in the SIZ reference grid.
    pub max_pixels: u64,
    /// Maximum component count declared by SIZ.
    pub max_components: u16,
    /// Maximum number of tiles implied by the SIZ tile grid.
    pub max_tiles: u32,
    /// Maximum temporary storage used while reconstructing a tile.
    pub max_working_bytes: usize,
}

/// Input interpretation and resource policy for a decoder instance.
///
/// The format selection is applied while parsing the Part 1 header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecoderOptions {
    /// Whether to detect or require a raw codestream or JP2 container.
    pub format: InputFormat,
    /// Caller-supplied bounds for parsing and tile reconstruction.
    pub limits: DecoderLimits,
}
