use crate::{
    Jbig2Error, Result,
    document::DecodeBudget,
    error::{LimitKind, SourceLocation},
};

// T.88 Annex A.1: Qe, next MPS state, next LPS state, and MPS switch.
const PROBABILITIES: [(u16, u8, u8, bool); 47] = [
    (0x5601, 1, 1, true),
    (0x3401, 2, 6, false),
    (0x1801, 3, 9, false),
    (0x0ac1, 4, 12, false),
    (0x0521, 5, 29, false),
    (0x0221, 38, 33, false),
    (0x5601, 7, 6, true),
    (0x5401, 8, 14, false),
    (0x4801, 9, 14, false),
    (0x3801, 10, 14, false),
    (0x3001, 11, 17, false),
    (0x2401, 12, 18, false),
    (0x1c01, 13, 20, false),
    (0x1601, 29, 21, false),
    (0x5601, 15, 14, true),
    (0x5401, 16, 14, false),
    (0x5101, 17, 15, false),
    (0x4801, 18, 16, false),
    (0x3801, 19, 17, false),
    (0x3401, 20, 18, false),
    (0x3001, 21, 19, false),
    (0x2801, 22, 19, false),
    (0x2401, 23, 20, false),
    (0x2201, 24, 21, false),
    (0x1c01, 25, 22, false),
    (0x1801, 26, 23, false),
    (0x1601, 27, 24, false),
    (0x1401, 28, 25, false),
    (0x1201, 29, 26, false),
    (0x1101, 30, 27, false),
    (0x0ac1, 31, 28, false),
    (0x09c1, 32, 29, false),
    (0x08a1, 33, 30, false),
    (0x0521, 34, 31, false),
    (0x0441, 35, 32, false),
    (0x02a1, 36, 33, false),
    (0x0221, 37, 34, false),
    (0x0141, 38, 35, false),
    (0x0111, 39, 36, false),
    (0x0085, 40, 37, false),
    (0x0049, 41, 38, false),
    (0x0025, 42, 39, false),
    (0x0015, 43, 40, false),
    (0x0009, 44, 41, false),
    (0x0005, 45, 42, false),
    (0x0001, 45, 43, false),
    (0x5601, 46, 46, false),
];

/// Result of integer entropy decoding, preserving T.88 out-of-band semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerValue {
    /// A decoded signed integer, widened to accommodate Huffman range offsets.
    Value(i64),
    /// An explicit out-of-band code, interpreted by the calling coding procedure.
    OutOfBand,
}

impl IntegerValue {
    /// Return the decoded value, rejecting an out-of-band result.
    ///
    /// Out of band is meaningful only where a coding procedure defines it as a
    /// terminator; every other field treats it as malformed data.
    pub fn value(self, what: &'static str) -> Result<i64> {
        match self {
            Self::Value(value) => Ok(value),
            Self::OutOfBand => Err(Jbig2Error::InvalidResource(what)),
        }
    }

    /// Return the decoded value narrowed to the width JBIG2 coordinates use.
    pub fn coordinate(self, what: &'static str) -> Result<i32> {
        i32::try_from(self.value(what)?).map_err(|_| Jbig2Error::Overflow(what))
    }
}

/// An integer decoding procedure of T.88 Annex A.
///
/// Each procedure keeps its own probability contexts, so decoding a width never
/// disturbs the state a height or an export run has built up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegerContext {
    /// `IADH`, symbol dictionary height class delta.
    DeltaHeight,
    /// `IADW`, symbol dictionary width delta.
    DeltaWidth,
    /// `IAEX`, symbol dictionary export run length.
    ExportRun,
    /// `IAAI`, symbol dictionary aggregate instance count.
    AggregateCount,
    /// `IADT`, text region strip delta.
    DeltaT,
    /// `IAFS`, text region first symbol S delta.
    FirstS,
    /// `IADS`, text region subsequent symbol S delta.
    DeltaS,
    /// `IAIT`, text region within-strip T offset.
    StripT,
    /// `IARI`, per-instance refinement flag.
    RefineFlag,
    /// `IARDW`, refinement width delta.
    RefineWidth,
    /// `IARDH`, refinement height delta.
    RefineHeight,
    /// `IARDX`, refinement horizontal displacement.
    RefineX,
    /// `IARDY`, refinement vertical displacement.
    RefineY,
}

impl IntegerContext {
    /// Position of this procedure's context bank.
    fn index(self) -> usize {
        match self {
            Self::DeltaHeight => 0,
            Self::DeltaWidth => 1,
            Self::ExportRun => 2,
            Self::AggregateCount => 3,
            Self::DeltaT => 4,
            Self::FirstS => 5,
            Self::DeltaS => 6,
            Self::StripT => 7,
            Self::RefineFlag => 8,
            Self::RefineWidth => 9,
            Self::RefineHeight => 10,
            Self::RefineX => 11,
            Self::RefineY => 12,
        }
    }
}

/// Number of independent integer procedures, excluding `IAID`.
const INTEGER_PROCEDURES: usize = 13;
/// Contexts one integer procedure addresses (T.88 section A.2).
const INTEGER_PROCEDURE_CONTEXTS: usize = 512;

/// Independent context banks for every integer procedure and for `IAID`.
///
/// A symbol dictionary may retain this whole bank for a later dictionary that
/// declares context reuse, which is why it is owned separately from a decoder.
#[derive(Debug)]
pub struct IntegerContexts {
    procedures: Vec<ArithmeticContexts>,
    identifier: ArithmeticContexts,
}

impl IntegerContexts {
    /// Allocate every bank, sizing `IAID` for identifiers of `code_bits` bits.
    pub fn new(code_bits: u8, budget: &mut DecodeBudget) -> Result<Self> {
        if code_bits > 31 {
            return Err(Jbig2Error::UnsupportedFeature(
                "IAID identifiers wider than 31 bits",
            ));
        }
        let identifier_contexts = 1_usize
            .checked_shl(u32::from(code_bits).saturating_add(1))
            .ok_or(Jbig2Error::Overflow("IAID contexts"))?;
        let mut procedures = Vec::new();
        procedures.try_reserve_exact(INTEGER_PROCEDURES)?;
        for _ in 0..INTEGER_PROCEDURES {
            procedures.push(ArithmeticContexts::new(INTEGER_PROCEDURE_CONTEXTS, budget)?);
        }
        Ok(Self {
            procedures,
            identifier: ArithmeticContexts::new(identifier_contexts, budget)?,
        })
    }

    /// Borrow one integer procedure's contexts.
    pub fn procedure(&mut self, which: IntegerContext) -> Result<&mut ArithmeticContexts> {
        self.procedures
            .get_mut(which.index())
            .ok_or(Jbig2Error::InvalidResource("integer context bank"))
    }

    /// Borrow the symbol identifier contexts.
    pub fn identifier(&mut self) -> &mut ArithmeticContexts {
        &mut self.identifier
    }
}

/// Owned probability states reused within, or retained across, coded segments.
///
/// Context ownership is explicit; symbol dictionaries may preserve it according
/// to their retain/use flags. This storage contains no input pointers.
#[derive(Debug)]
pub struct ArithmeticContexts {
    states: Vec<u8>,
}

impl ArithmeticContexts {
    /// Allocate initial states, checking the context count and shared budget.
    pub fn new(count: usize, budget: &mut DecodeBudget) -> Result<Self> {
        budget.charge(
            LimitKind::AllocationBytes,
            u64::try_from(count).map_err(|_| Jbig2Error::Overflow("arithmetic contexts"))?,
        )?;
        let mut states = Vec::new();
        states.try_reserve_exact(count)?;
        states.resize(count, 0);
        Ok(Self { states })
    }

    /// Number of contexts this bank addresses.
    pub(crate) fn len(&self) -> usize {
        self.states.len()
    }

    /// Copy these states into independently owned storage, charging the
    /// budget as a fresh bank would.
    pub(crate) fn duplicate(&self, budget: &mut DecodeBudget) -> Result<Self> {
        let mut copy = Self::new(self.states.len(), budget)?;
        copy.states.copy_from_slice(&self.states);
        Ok(copy)
    }

    /// Use these states as the bank a procedure needs, or allocate fresh
    /// states when none were inherited; an inherited bank must be the size
    /// the procedure's template addresses.
    pub(crate) fn inherit(
        inherited: Option<Self>,
        count: usize,
        budget: &mut DecodeBudget,
    ) -> Result<Self> {
        match inherited {
            Some(bank) if bank.len() == count => Ok(bank),
            Some(_) => Err(Jbig2Error::InvalidResource(
                "retained contexts for a different template",
            )),
            None => Self::new(count, budget),
        }
    }
}

/// T.88 Annex E arithmetic decoder borrowing a bounded encoded payload.
///
/// Mutable contexts are supplied per call rather than stored as aliases. An
/// adapter must retain input storage until this decoder is dropped.
#[derive(Debug)]
pub struct ArithmeticDecoder<'a> {
    input: &'a [u8],
    location: SourceLocation,
    position: usize,
    interval: u32,
    code: u32,
    current_byte: u8,
    bit_count: u32,
}

impl<'a> ArithmeticDecoder<'a> {
    /// Initialize from bounded bytes; report truncation or invalid coding data.
    pub fn new(input: &'a [u8], location: SourceLocation) -> Result<Self> {
        let current_byte = *input.first().ok_or(Jbig2Error::Truncated {
            location,
            needed: 1,
        })?;
        let mut decoder = Self {
            input,
            location,
            position: 0,
            interval: 0x8000,
            code: u32::from(current_byte ^ 0xff) << 16,
            current_byte,
            bit_count: 0,
        };
        decoder.byte_in();
        decoder.code = decoder.code.wrapping_shl(7);
        decoder.bit_count = decoder.bit_count.saturating_sub(7);
        Ok(decoder)
    }

    /// Decode one context-selected decision and charge entropy work.
    pub fn decode_bit(
        &mut self,
        contexts: &mut ArithmeticContexts,
        index: usize,
        budget: &mut DecodeBudget,
    ) -> Result<bool> {
        budget.charge(LimitKind::Work, 1)?;
        let state = *contexts
            .states
            .get(index)
            .ok_or(Jbig2Error::InvalidCodingData {
                location: self.location,
                reason: "arithmetic context index",
            })?;
        let index_in_table = usize::from(state & 0x7f);
        let (qe, nmps, nlps, switch) =
            *PROBABILITIES
                .get(index_in_table)
                .ok_or(Jbig2Error::InvalidCodingData {
                    location: self.location,
                    reason: "arithmetic probability state",
                })?;
        let mps = state & 0x80 != 0;
        self.interval = self.interval.wrapping_sub(u32::from(qe));
        let (decoded, lps, renorm) = if (self.code >> 16) < self.interval {
            if self.interval & 0x8000 != 0 {
                (mps, false, false)
            } else {
                let lps = self.interval < u32::from(qe);
                (mps ^ lps, lps, true)
            }
        } else {
            self.code = self.code.wrapping_sub(self.interval << 16);
            let lps = self.interval >= u32::from(qe);
            self.interval = u32::from(qe);
            (mps ^ lps, lps, true)
        };
        if renorm {
            let next_mps = mps ^ (lps && switch);
            let next_state = if lps { nlps } else { nmps };
            let slot = contexts
                .states
                .get_mut(index)
                .ok_or(Jbig2Error::InvalidCodingData {
                    location: self.location,
                    reason: "arithmetic context index",
                })?;
            *slot = next_state | if next_mps { 0x80 } else { 0 };
            while self.interval & 0x8000 == 0 {
                if self.bit_count == 0 {
                    self.byte_in();
                }
                self.interval = self.interval.wrapping_shl(1);
                self.code = self.code.wrapping_shl(1);
                self.bit_count = self.bit_count.saturating_sub(1);
            }
        }
        Ok(decoded)
    }

    /// Decode an arithmetic integer with its own procedure-specific contexts.
    pub fn decode_integer(
        &mut self,
        contexts: &mut ArithmeticContexts,
        budget: &mut DecodeBudget,
    ) -> Result<IntegerValue> {
        if contexts.states.len() < 512 {
            return Err(Jbig2Error::InvalidCodingData {
                location: self.location,
                reason: "arithmetic integer context count",
            });
        }
        let mut previous = 1_usize;
        let sign = self.integer_bit(contexts, &mut previous, budget)?;
        let ranges = [
            (2_u8, 0_u64),
            (4, 4),
            (6, 20),
            (8, 84),
            (12, 340),
            (32, 4436),
        ];
        let mut selected = ranges.len().saturating_sub(1);
        for index in 0..ranges.len() {
            selected = index;
            if index == ranges.len().saturating_sub(1)
                || !self.integer_bit(contexts, &mut previous, budget)?
            {
                break;
            }
        }
        let (bits, offset) = *ranges.get(selected).ok_or(Jbig2Error::InvalidCodingData {
            location: self.location,
            reason: "integer range",
        })?;
        let mut raw = 0_u64;
        for _ in 0..bits {
            raw = (raw << 1) | u64::from(self.integer_bit(contexts, &mut previous, budget)?);
        }
        let value = offset
            .checked_add(raw)
            .ok_or(Jbig2Error::Overflow("arithmetic integer"))?;
        if sign && value == 0 {
            return Ok(IntegerValue::OutOfBand);
        }
        let signed =
            i64::try_from(value).map_err(|_| Jbig2Error::Overflow("arithmetic integer"))?;
        Ok(IntegerValue::Value(if sign {
            signed
                .checked_neg()
                .ok_or(Jbig2Error::Overflow("arithmetic integer"))?
        } else {
            signed
        }))
    }

    /// Decode an IAID symbol identifier using the supplied code length.
    pub fn decode_symbol_id(
        &mut self,
        contexts: &mut ArithmeticContexts,
        bits: u8,
        budget: &mut DecodeBudget,
    ) -> Result<u32> {
        if bits > 31 {
            return Err(Jbig2Error::UnsupportedFeature(
                "IAID identifiers wider than 31 bits",
            ));
        }
        let required = 1_usize
            .checked_shl(u32::from(bits) + 1)
            .ok_or(Jbig2Error::Overflow("IAID contexts"))?;
        if contexts.states.len() < required {
            return Err(Jbig2Error::InvalidCodingData {
                location: self.location,
                reason: "IAID context count",
            });
        }
        let mut previous = 1_usize;
        for _ in 0..bits {
            let bit = usize::from(self.decode_bit(contexts, previous, budget)?);
            previous = previous
                .checked_shl(1)
                .and_then(|value| value.checked_add(bit))
                .ok_or(Jbig2Error::Overflow("IAID value"))?;
        }
        let mask = 1_usize
            .checked_shl(u32::from(bits))
            .and_then(|value| value.checked_sub(1))
            .ok_or(Jbig2Error::Overflow("IAID mask"))?;
        u32::try_from(previous & mask).map_err(|_| Jbig2Error::Overflow("IAID identifier"))
    }

    /// Return how many input bytes this decoder has read.
    ///
    /// Arithmetic coding terminates on a decision count rather than a marker,
    /// so a caller that must resume at an exact boundary derives it from its own
    /// declared payload length, not from this count.
    pub fn consumed_bytes(&self) -> usize {
        self.position.saturating_add(1).min(self.input.len())
    }

    fn integer_bit(
        &mut self,
        contexts: &mut ArithmeticContexts,
        previous: &mut usize,
        budget: &mut DecodeBudget,
    ) -> Result<bool> {
        let bit = self.decode_bit(contexts, *previous, budget)?;
        let next = previous
            .checked_mul(2)
            .and_then(|value| value.checked_add(usize::from(bit)))
            .ok_or(Jbig2Error::Overflow("integer context"))?;
        *previous = if *previous < 256 {
            next
        } else {
            (next & 511) | 256
        };
        Ok(bit)
    }

    fn byte_in(&mut self) {
        if self.current_byte == 0xff {
            let next = self
                .input
                .get(self.position.saturating_add(1))
                .copied()
                .unwrap_or(0xff);
            if next > 0x8f {
                self.bit_count = 8;
            } else {
                self.position = self.position.saturating_add(1).min(self.input.len());
                self.current_byte = next;
                self.code = self
                    .code
                    .wrapping_add(0xfe00)
                    .wrapping_sub(u32::from(next) << 9);
                self.bit_count = 7;
            }
        } else {
            self.position = self.position.saturating_add(1).min(self.input.len());
            let next = self.input.get(self.position).copied().unwrap_or(0xff);
            self.current_byte = next;
            self.code = self
                .code
                .wrapping_add(0xff00)
                .wrapping_sub(u32::from(next) << 8);
            self.bit_count = 8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ArithmeticContexts, ArithmeticDecoder};
    use crate::{
        Result,
        document::{DecodeBudget, DecodeLimits},
        error::{InputOrigin, SourceLocation},
    };

    fn budget() -> DecodeBudget {
        DecodeBudget::new(DecodeLimits {
            input_bytes: 1000,
            segments: 100,
            references: 100,
            pixels: 100,
            allocation_bytes: 4096,
            symbols: 100,
            work: 1000,
        })
    }

    fn location() -> SourceLocation {
        SourceLocation {
            origin: InputOrigin::Embedded,
            byte_offset: 0,
            bit_offset: None,
        }
    }

    #[test]
    fn decision_sequence_and_context_updates_are_deterministic() -> Result<()> {
        let mut budget = budget();
        let mut contexts = ArithmeticContexts::new(1, &mut budget)?;
        let mut decoder = ArithmeticDecoder::new(&[0, 0, 0], location())?;
        let bits = (0..8)
            .map(|_| decoder.decode_bit(&mut contexts, 0, &mut budget))
            .collect::<Result<Vec<_>>>()?;
        assert_eq!(
            bits,
            vec![false, true, true, false, true, false, true, false]
        );
        assert!(contexts.states.first().is_some_and(|state| *state < 47));
        assert!(decoder.consumed_bytes() <= 3);
        Ok(())
    }

    #[test]
    fn empty_input_and_missing_context_fail() -> Result<()> {
        assert!(ArithmeticDecoder::new(&[], location()).is_err());
        let mut budget = budget();
        let mut contexts = ArithmeticContexts::new(0, &mut budget)?;
        let mut decoder = ArithmeticDecoder::new(&[0], location())?;
        assert!(decoder.decode_bit(&mut contexts, 0, &mut budget).is_err());
        assert!(decoder.decode_integer(&mut contexts, &mut budget).is_err());
        Ok(())
    }
}
