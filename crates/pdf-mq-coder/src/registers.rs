//! The interval and code registers of the MQ arithmetic decoder.
//!
//! This is the decoding procedure itself: `INITDEC` in [`MqRegisters::new`],
//! `DECODE` with its two exchange procedures in [`MqRegisters::decode`], and
//! `RENORMD` and `BYTEIN` beneath them.

use crate::{byte_source::MqByteSource, context::MqContext, error::MqError, probability::QE_TABLE};

/// Initial and renormalization high bit of the interval register.
const DEFAULT_INTERVAL: u32 = 0x8000;
/// Shift separating the code register's comparand from its fraction.
const CODE_REGISTER_SHIFT: u32 = 16;
/// Final initialization shift, applied after the first `BYTEIN`.
const POST_BYTE_IN_CODE_SHIFT: u32 = 7;
/// Byte value that introduces a possible marker.
const MARKER_PREFIX: u8 = 0xff;
/// Largest byte that may follow `0xff` without being a marker.
const MARKER_BYTE_THRESHOLD: u8 = 0x8f;
/// Code-register adjustment after an ordinary byte.
const NORMAL_BYTE_CODE_OFFSET: u32 = 0xff00;
/// Code-register adjustment after a byte following `0xff`.
const STUFFED_BYTE_CODE_OFFSET: u32 = 0xfe00;
/// Bit count loaded after an ordinary byte.
const NORMAL_BYTE_BIT_COUNT: u32 = 8;
/// Bit count loaded after a byte following `0xff`.
const STUFFED_BYTE_BIT_COUNT: u32 = 7;
/// Code-register shift for an ordinary byte.
const NORMAL_BYTE_CODE_SHIFT: u32 = 8;
/// Code-register shift for a byte following `0xff`.
const STUFFED_BYTE_CODE_SHIFT: u32 = 9;

/// The register state of one MQ arithmetic decoding run.
///
/// The registers are separate from the byte source so a caller can keep the
/// source in whatever form its own stream needs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MqRegisters {
    code: u32,
    interval: u32,
    bit_count: u32,
    current_byte: u8,
    exhausted: bool,
}

impl MqRegisters {
    /// Initializes the registers from the source's current position.
    ///
    /// A source with no byte at all yields registers that refuse to decode,
    /// which keeps the truncated case out of the decoding loop.
    pub fn new(source: &mut impl MqByteSource) -> Self {
        let available = source.peek().is_some();
        let current_byte = source.peek().unwrap_or(MARKER_PREFIX);
        let mut registers = Self {
            code: u32::from(current_byte ^ MARKER_PREFIX).wrapping_shl(CODE_REGISTER_SHIFT),
            interval: DEFAULT_INTERVAL,
            bit_count: 0,
            current_byte,
            exhausted: !available,
        };
        registers.byte_in(source);
        registers.code = registers.code.wrapping_shl(POST_BYTE_IN_CODE_SHIFT);
        registers.bit_count = registers.bit_count.saturating_sub(POST_BYTE_IN_CODE_SHIFT);
        registers
    }

    /// Returns whether initialization found no arithmetic byte at all.
    pub fn is_exhausted(self) -> bool {
        self.exhausted
    }

    /// Decodes one binary decision against an adaptive context.
    ///
    /// # Errors
    ///
    /// Returns `Truncated` when the run was initialized without data, and
    /// `InvalidState` when the context names a state outside the table.
    pub fn decode(
        &mut self,
        source: &mut impl MqByteSource,
        context: &mut MqContext,
    ) -> Result<bool, MqError> {
        if self.exhausted {
            return Err(MqError::Truncated);
        }
        let state = QE_TABLE
            .get(usize::from(context.probability_index()))
            .ok_or(MqError::InvalidState)?;
        self.interval = self.interval.wrapping_sub(state.qe);
        if self.code.wrapping_shr(CODE_REGISTER_SHIFT) < self.interval {
            if self.interval & DEFAULT_INTERVAL != 0 {
                return Ok(context.mps());
            }
            let decoded = if self.interval < state.qe {
                context.decode_nlps(state)
            } else {
                context.decode_nmps(state)
            };
            self.renormalize(source);
            return Ok(decoded);
        }
        self.code = self
            .code
            .wrapping_sub(self.interval.wrapping_shl(CODE_REGISTER_SHIFT));
        let decoded = if self.interval < state.qe {
            context.decode_nmps(state)
        } else {
            context.decode_nlps(state)
        };
        self.interval = state.qe;
        self.renormalize(source);
        Ok(decoded)
    }

    /// Renormalizes the interval and code registers.
    fn renormalize(&mut self, source: &mut impl MqByteSource) {
        while self.interval & DEFAULT_INTERVAL == 0 {
            if self.bit_count == 0 {
                self.byte_in(source);
            }
            self.interval = self.interval.wrapping_shl(1);
            self.code = self.code.wrapping_shl(1);
            self.bit_count = self.bit_count.saturating_sub(1);
        }
    }

    /// Loads the next byte, keeping a marker out of the arithmetic data.
    fn byte_in(&mut self, source: &mut impl MqByteSource) {
        if self.current_byte == MARKER_PREFIX {
            self.byte_in_after_marker_prefix(source);
        } else {
            self.byte_in_regular(source);
        }
    }

    /// Loads the byte that follows `0xff`, stopping at a real marker.
    fn byte_in_after_marker_prefix(&mut self, source: &mut impl MqByteSource) {
        let next = source.peek_next().unwrap_or(MARKER_PREFIX);
        if next > MARKER_BYTE_THRESHOLD {
            self.bit_count = NORMAL_BYTE_BIT_COUNT;
            return;
        }
        source.advance();
        self.current_byte = next;
        self.code = self
            .code
            .wrapping_add(STUFFED_BYTE_CODE_OFFSET)
            .wrapping_sub(u32::from(self.current_byte).wrapping_shl(STUFFED_BYTE_CODE_SHIFT));
        self.bit_count = STUFFED_BYTE_BIT_COUNT;
    }

    /// Loads the byte that follows ordinary arithmetic data.
    fn byte_in_regular(&mut self, source: &mut impl MqByteSource) {
        source.advance();
        self.current_byte = source.peek().unwrap_or(MARKER_PREFIX);
        self.code = self
            .code
            .wrapping_add(NORMAL_BYTE_CODE_OFFSET)
            .wrapping_sub(u32::from(self.current_byte).wrapping_shl(NORMAL_BYTE_CODE_SHIFT));
        self.bit_count = NORMAL_BYTE_BIT_COUNT;
    }
}
