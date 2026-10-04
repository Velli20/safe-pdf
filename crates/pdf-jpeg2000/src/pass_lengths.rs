//! Coding-pass counts and codeword-segment lengths in a packet header.
//!
//! Annex B.10.6 codes the number of new coding passes a code-block
//! contributes, and Annex B.10.7 codes the byte length of each codeword
//! segment those passes occupy. How many segments a contribution splits into
//! follows from the code-block style: `TERMINATE` ends every pass, and
//! `BYPASS` ends the arithmetic run and then alternates raw and arithmetic
//! segments, as Annex D.6 describes.

use crate::{
    Jpeg2000Error,
    coding::CodeBlockFlags,
    stuffed_bits::{HeaderSource, StuffedBitReader},
};

/// Coding passes that always precede the first selective-bypass pass.
const BYPASS_FIRST_RAW_PASS: u32 = 10;
/// Coding passes in one bit-plane above the first cleanup pass.
const PASSES_PER_PLANE: u32 = 3;
/// Raw coding passes grouped into one bypass codeword segment.
const BYPASS_RAW_PASSES: u32 = 2;
/// Largest pass count Table B.4 can signal.
const MAX_PASSES: u32 = 164;
/// Passes signalled by the two-bit field of Table B.4.
const SHORT_FIELD_BITS: u32 = 2;
/// Passes signalled by the five-bit field of Table B.4.
const MEDIUM_FIELD_BITS: u32 = 5;
/// Passes signalled by the seven-bit field of Table B.4.
const LONG_FIELD_BITS: u32 = 7;
/// First pass count reached through the two-bit field.
const SHORT_FIELD_BASE: u32 = 3;
/// First pass count reached through the five-bit field.
const MEDIUM_FIELD_BASE: u32 = 6;
/// First pass count reached through the seven-bit field.
const LONG_FIELD_BASE: u32 = 37;
/// Escape value of the two-bit field.
const SHORT_FIELD_ESCAPE: u32 = 3;
/// Escape value of the five-bit field.
const MEDIUM_FIELD_ESCAPE: u32 = 31;
/// Initial value of a code-block's `Lblock` length-prefix width.
const INITIAL_LENGTH_BITS: u8 = 3;
/// Largest `Lblock` growth a single packet may signal.
const MAX_LENGTH_BITS_GROWTH: u32 = 32;

/// Reads the Table B.4 code for the number of new coding passes.
///
/// # Errors
///
/// Returns a structural error if the header is truncated.
pub(crate) fn read_pass_count<S: HeaderSource>(
    reader: &mut StuffedBitReader<S>,
) -> Result<u32, Jpeg2000Error> {
    if !reader.read_bit()? {
        return Ok(1);
    }
    if !reader.read_bit()? {
        return Ok(2);
    }
    let short = reader.read_bits(SHORT_FIELD_BITS)?;
    if short < SHORT_FIELD_ESCAPE {
        return Ok(SHORT_FIELD_BASE.saturating_add(short));
    }
    let medium = reader.read_bits(MEDIUM_FIELD_BITS)?;
    if medium < MEDIUM_FIELD_ESCAPE {
        return Ok(MEDIUM_FIELD_BASE.saturating_add(medium));
    }
    let long = reader.read_bits(LONG_FIELD_BITS)?;
    Ok(LONG_FIELD_BASE.saturating_add(long).min(MAX_PASSES))
}

/// Reads the `Lblock` increase signalled by a unary run of one-bits.
///
/// # Errors
///
/// Returns a structural error if the header is truncated, and `Overflow` if
/// the run would make the length field wider than a `u32`.
pub(crate) fn read_length_bits<S: HeaderSource>(
    reader: &mut StuffedBitReader<S>,
    current: u8,
) -> Result<u8, Jpeg2000Error> {
    let growth = reader.read_unary(MAX_LENGTH_BITS_GROWTH)?;
    let widened = u32::from(current)
        .checked_add(growth)
        .filter(|bits| *bits < u32::BITS)
        .ok_or(Jpeg2000Error::Overflow {
            context: "code-block length prefix width",
        })?;
    u8::try_from(widened).map_err(|_| Jpeg2000Error::Overflow {
        context: "code-block length prefix width",
    })
}

/// Returns the initial `Lblock` width of a code-block.
pub(crate) fn initial_length_bits() -> u8 {
    INITIAL_LENGTH_BITS
}

/// Splits a contribution's coding passes into codeword segments.
///
/// The split depends only on the code-block style and on where the passes sit
/// in the block's overall pass sequence, so the same rule applies in every
/// layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SegmentSplit {
    style: CodeBlockFlags,
}

impl SegmentSplit {
    /// Builds the split rule for one code-block style.
    pub(crate) fn new(style: CodeBlockFlags) -> Self {
        Self { style }
    }

    /// Returns whether each codeword segment ends after a single pass.
    fn terminates_every_pass(self) -> bool {
        self.style.contains(CodeBlockFlags::TERMINATE)
    }

    /// Returns whether arithmetic coding is bypassed above the fourth plane.
    fn bypasses(self) -> bool {
        self.style.contains(CodeBlockFlags::BYPASS)
    }

    /// Returns the passes of the segment that starts at pass index `start`.
    ///
    /// The result never exceeds `remaining`, because a contribution may end
    /// part-way through a segment that continues in a later layer.
    pub(crate) fn segment_passes(self, start: u32, remaining: u32) -> u32 {
        if remaining == 0 {
            return 0;
        }
        let passes = if self.terminates_every_pass() {
            1
        } else if !self.bypasses() {
            remaining
        } else if start < BYPASS_FIRST_RAW_PASS {
            BYPASS_FIRST_RAW_PASS.saturating_sub(start)
        } else if start
            .saturating_sub(BYPASS_FIRST_RAW_PASS)
            .checked_rem(PASSES_PER_PLANE)
            == Some(0)
        {
            BYPASS_RAW_PASSES
        } else {
            1
        };
        passes.min(remaining)
    }

    /// Returns every segment pass count of one contribution, in order.
    pub(crate) fn segments(self, start: u32, passes: u32) -> impl Iterator<Item = u32> {
        let mut position = start;
        let mut remaining = passes;
        core::iter::from_fn(move || {
            let count = self.segment_passes(position, remaining);
            if count == 0 {
                return None;
            }
            position = position.saturating_add(count);
            remaining = remaining.saturating_sub(count);
            Some(count)
        })
    }
}

/// Reads one codeword segment's byte length from a packet header.
///
/// Annex B.10.7.1 widens the length field by the base-two logarithm of the
/// passes the segment carries.
///
/// # Errors
///
/// Returns a structural error if the header is truncated, and `Overflow` if
/// the field would be wider than a `u32`.
pub(crate) fn read_segment_length<S: HeaderSource>(
    reader: &mut StuffedBitReader<S>,
    length_bits: u8,
    passes: u32,
) -> Result<usize, Jpeg2000Error> {
    let width = u32::from(length_bits)
        .checked_add(passes.max(1).ilog2())
        .filter(|bits| *bits <= u32::BITS)
        .ok_or(Jpeg2000Error::Overflow {
            context: "codeword segment length field",
        })?;
    let length = reader.read_bits(width)?;
    usize::try_from(length).map_err(|_| Jpeg2000Error::Overflow {
        context: "codeword segment length",
    })
}
