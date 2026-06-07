//! JBIG2 text-region decode working state.
//!
//! This module models the text-region procedure working variables from
//! ITU-T T.88 | ISO/IEC 14492 section 6.4.5. The state tracks `STRIPT`,
//! `FIRSTS`, `CURS`, and `NINSTANCES` while callers decode symbol IDs,
//! lookup bitmaps, and compose symbol instances.

use crate::{error::Jbig2Error, util::INTEGER_CONVERSION_OVERFLOW};

/// Decode state for the JBIG2 text-region procedure.
///
/// The fields correspond to the working variables named in ITU-T T.88 |
/// ISO/IEC 14492 section 6.4.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextRegionDecodeState {
    stript: i64,
    firsts: i64,
    strip_first_symbol_pending: bool,
    strip_curs: i64,
    instances: u32,
}

impl TextRegionDecodeState {
    /// Initialize the JBIG2 text-region working variables.
    ///
    /// This implements ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 2 using
    /// the initial strip delta decoded by section 6.4.6.
    pub(crate) fn from_initial_delta(
        initial_delta_t: i32,
        sbstrips: u8,
    ) -> Result<Self, Jbig2Error> {
        let stript = i64::from(initial_delta_t)
            .checked_mul(i64::from(sbstrips))
            .and_then(|value| value.checked_neg())
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;

        Ok(Self {
            stript,
            firsts: 0,
            strip_first_symbol_pending: true,
            strip_curs: 0,
            instances: 0,
        })
    }

    /// Return whether all `SBNUMINSTANCES` symbol instances have been decoded.
    ///
    /// This matches ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(a).
    pub(crate) fn is_complete(self, symbol_instances: u32) -> bool {
        self.instances >= symbol_instances
    }

    /// Increment `NINSTANCES` after composing one symbol instance.
    ///
    /// This implements ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c)xi.
    pub(crate) fn record_instance(&mut self) -> Result<(), Jbig2Error> {
        self.instances = self
            .instances
            .checked_add(1)
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        Ok(())
    }

    /// Advance `STRIPT` by one decoded strip delta.
    ///
    /// This implements ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(b)
    /// using the strip delta coding rule from section 6.4.6.
    pub(crate) fn advance_strip(&mut self, delta_t: i32, sbstrips: u8) -> Result<i64, Jbig2Error> {
        let scaled_delta_t = i64::from(delta_t)
            .checked_mul(i64::from(sbstrips))
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        self.stript = self
            .stript
            .checked_add(scaled_delta_t)
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        self.start_huffman_strip();
        Ok(self.stript)
    }

    /// Consume the first-symbol `S` delta for a strip.
    ///
    /// This implements ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c)i
    /// and section 6.4.7 by updating `FIRSTS = FIRSTS + DFS`.
    pub(crate) fn consume_first_s_delta(&mut self, delta_first_s: i32) -> Result<i64, Jbig2Error> {
        self.firsts = self
            .firsts
            .checked_add(i64::from(delta_first_s))
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        Ok(self.firsts)
    }

    /// Consume the first-symbol Huffman `S` delta and seed `CURS`.
    ///
    /// This is the Huffman realization of ITU-T T.88 | ISO/IEC 14492 section
    /// 6.4.5 step 3(c)i.
    pub(crate) fn consume_huffman_first_s(&mut self, dfs: i32) -> Result<(), Jbig2Error> {
        self.strip_curs = self.consume_first_s_delta(dfs)?;
        self.strip_first_symbol_pending = false;
        Ok(())
    }

    /// Consume a subsequent-symbol `S` delta for the Huffman path.
    ///
    /// This implements ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c)ii
    /// and section 6.4.8 by applying `CURS = CURS + IDS + SBDSOFFSET`.
    pub(crate) fn consume_huffman_delta_s(
        &mut self,
        delta_s: i32,
        sbdsoffset: i8,
    ) -> Result<(), Jbig2Error> {
        self.strip_curs = advance_curs_by_delta_s(self.strip_curs, delta_s, sbdsoffset)?;
        Ok(())
    }

    /// Return the current `STRIPT` value from section 6.4.5.
    pub(crate) fn stript(self) -> i64 {
        self.stript
    }

    /// Return the current strip `CURS` value for the Huffman path.
    pub(crate) fn strip_curs(self) -> i64 {
        self.strip_curs
    }

    /// Return whether the next Huffman symbol is the first symbol in its strip.
    pub(crate) fn strip_first_symbol_pending(self) -> bool {
        self.strip_first_symbol_pending
    }

    /// Reset per-strip Huffman placement state.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(b) starts a new strip
    /// after each decoded `DT` value.
    fn start_huffman_strip(&mut self) {
        self.strip_first_symbol_pending = true;
        self.strip_curs = 0;
    }
}

/// Apply the text-region subsequent-symbol `S` update.
///
/// This helper implements `CURS = CURS + IDS + SBDSOFFSET` from ITU-T T.88 |
/// ISO/IEC 14492 section 6.4.5 step 3(c)ii.
pub(crate) fn advance_curs_by_delta_s(
    curs: i64,
    delta_s: i32,
    sbdsoffset: i8,
) -> Result<i64, Jbig2Error> {
    curs.checked_add(i64::from(delta_s))
        .and_then(|value| value.checked_add(i64::from(sbdsoffset)))
        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

#[cfg(test)]
mod tests {
    use super::{TextRegionDecodeState, advance_curs_by_delta_s};
    use crate::error::Jbig2Error;

    #[test]
    fn state_advances_strip_by_sbstrips() -> Result<(), Jbig2Error> {
        let mut state = TextRegionDecodeState::from_initial_delta(2, 3)?;

        assert_eq!(state.stript(), -6);
        assert_eq!(state.advance_strip(4, 3)?, 6);

        Ok(())
    }

    #[test]
    fn state_tracks_huffman_first_and_delta_s() -> Result<(), Jbig2Error> {
        let mut state = TextRegionDecodeState::from_initial_delta(0, 1)?;

        state.consume_huffman_first_s(5)?;
        assert_eq!(state.consume_first_s_delta(0)?, 5);
        assert_eq!(state.strip_curs(), 5);
        assert!(!state.strip_first_symbol_pending());

        state.consume_huffman_delta_s(3, 2)?;
        assert_eq!(state.strip_curs(), 10);

        Ok(())
    }

    #[test]
    fn state_records_completion_at_symbol_instance_count() -> Result<(), Jbig2Error> {
        let mut state = TextRegionDecodeState::from_initial_delta(0, 1)?;

        assert!(!state.is_complete(2));
        state.record_instance()?;
        assert!(!state.is_complete(2));
        state.record_instance()?;
        assert!(state.is_complete(2));
        Ok(())
    }

    #[test]
    fn advance_curs_by_delta_s_applies_signed_offset() -> Result<(), Jbig2Error> {
        assert_eq!(advance_curs_by_delta_s(9, 3, -2)?, 10);
        Ok(())
    }
}
