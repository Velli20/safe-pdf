//! Arithmetic integer decoding for JBIG2 streams.
//!
//! ITU-T T.88 / ISO/IEC 14492 Annex A.2 defines a shared procedure for
//! decoding signed arithmetic integers except IAID. The integer classes below
//! identify the separate adaptive context pools used by JBIG2 segment
//! procedures.

use crate::arith_decoder::{
    bit_reader::{ArithmeticBitReader, BitContextSource},
    decoder::JBig2ArithDecoder,
};
use crate::error::Jbig2Error;

/// Number of contexts in each T.88 Annex A.2 arithmetic integer context pool.
pub(super) const ARITH_INT_CONTEXT_COUNT: usize = 1 << 9;

/// One row from the T.88 Annex A.2 integer decoding range table.
#[derive(Debug, Clone, Copy)]
pub(super) struct IntegerRange {
    /// Number of leading range-selection bits consumed before this row.
    pub(super) prefix_bits: u8,
    /// Number of payload bits used by this row.
    pub(super) value_bits: u8,
    /// Value offset added to the payload bits for this row.
    pub(super) offset: u32,
}

/// T.88 Annex A.2 integer ranges for arithmetic integers other than IAID.
pub(super) const INTEGER_RANGES: [IntegerRange; 6] = [
    IntegerRange {
        prefix_bits: 0,
        value_bits: 2,
        offset: 0,
    },
    IntegerRange {
        prefix_bits: 1,
        value_bits: 4,
        offset: 4,
    },
    IntegerRange {
        prefix_bits: 2,
        value_bits: 6,
        offset: 20,
    },
    IntegerRange {
        prefix_bits: 3,
        value_bits: 8,
        offset: 84,
    },
    IntegerRange {
        prefix_bits: 4,
        value_bits: 12,
        offset: 340,
    },
    IntegerRange {
        prefix_bits: 5,
        value_bits: 32,
        offset: 4436,
    },
];

/// JBIG2 arithmetic integer context class.
///
/// Each variant maps to one adaptive context pool used by a JBIG2 procedure
/// that calls the T.88 Annex A.2 arithmetic integer decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JBig2ArithIntegerContext {
    /// `IADT`: text-region delta `T` integers from T.88 section 6.4.5.
    TextDeltaT,
    /// `IAFS`: text-region first `S` integers from T.88 section 6.4.5.
    TextFirstS,
    /// `IAIT`: text-region instance `T` integers from T.88 section 6.4.5.
    TextInstanceT,
    /// `IADS`: text-region delta `S` integers from T.88 section 6.4.5.
    TextDeltaS,
    /// `IADH`: symbol-dictionary height deltas from T.88 section 6.5.5.
    SymbolHeightDelta,
    /// `IADW`: symbol-dictionary width deltas from T.88 section 6.5.5.
    SymbolWidthDelta,
    /// `IAEX`: symbol export run lengths from T.88 section 6.5.8.
    SymbolExportRunLength,
    /// `IAAI`: refinement aggregate instance counts from T.88 refinement procedures.
    RefinementAggregateInstances,
    /// `IARDW`: refinement bitmap width deltas.
    RefinementDeltaWidth,
    /// `IARDH`: refinement bitmap height deltas.
    RefinementDeltaHeight,
    /// `IARDX`: refinement bitmap x deltas.
    RefinementDeltaX,
    /// `IARDY`: refinement bitmap y deltas.
    RefinementDeltaY,
    /// `IARI`: text-region refinement flags.
    RefinementInstance,
}

impl JBig2ArithDecoder<'_, '_> {
    /// Decode a JBIG2 arithmetic integer using T.88 Annex A.2.
    ///
    /// `None` is returned for the arithmetic integer out-of-band marker;
    /// otherwise the decoded signed integer is returned.
    pub(crate) fn decode_integer(
        &mut self,
        context: JBig2ArithIntegerContext,
    ) -> Result<Option<i32>, Jbig2Error> {
        let mut bits = ArithmeticBitReader::new(self, BitContextSource::Integer(context));
        let sign = bits.read_integer_bits(1)?;
        let mut selected = *INTEGER_RANGES
            .last()
            .ok_or(Jbig2Error::InvalidState("arithmetic integer ranges"))?;

        for range in INTEGER_RANGES {
            selected = range;
            if range.prefix_bits == 5 {
                break;
            }
            if bits.read_integer_bits(1)? == 0 {
                break;
            }
        }

        let value = bits
            .read_integer_bits(selected.value_bits)?
            .saturating_add(selected.offset);
        let signed = if sign == 0 {
            i64::from(value)
        } else if value == 0 {
            return Ok(None);
        } else {
            i64::from(value)
                .checked_neg()
                .ok_or(Jbig2Error::Overflow("arithmetic integer overflow"))?
        };

        if signed < i64::from(i32::MIN) || signed > i64::from(i32::MAX) {
            return Ok(None);
        }

        Ok(Some(i32::try_from(signed).map_err(|_| {
            Jbig2Error::Overflow("arithmetic integer overflow")
        })?))
    }

    /// Decode a required JBIG2 arithmetic integer using T.88 Annex A.2.
    ///
    /// If the stream carries the arithmetic integer out-of-band marker, it is
    /// mapped to `Jbig2Error::InvalidState(label)`.
    pub(crate) fn decode_required_integer(
        &mut self,
        context: JBig2ArithIntegerContext,
        label: &'static str,
    ) -> Result<i32, Jbig2Error> {
        self.decode_integer(context)?
            .ok_or(Jbig2Error::InvalidState(label))
    }
}
