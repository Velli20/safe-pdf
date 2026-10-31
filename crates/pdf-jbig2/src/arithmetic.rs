use pdf_mq_coder::{MqContext, MqRegisters, SliceSource};

use crate::{
    Jbig2Error, Result,
    document::DecodeBudget,
    error::{LimitKind, SourceLocation},
};

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
    states: Vec<MqContext>,
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
        states.resize(count, MqContext::default());
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
    source: SliceSource<'a>,
    registers: MqRegisters,
    location: SourceLocation,
}

impl<'a> ArithmeticDecoder<'a> {
    /// Initialize from bounded bytes; report truncation or invalid coding data.
    pub fn new(input: &'a [u8], location: SourceLocation) -> Result<Self> {
        let mut source = SliceSource::new(input);
        let registers = MqRegisters::new(&mut source);
        if registers.is_exhausted() {
            return Err(Jbig2Error::Truncated {
                location,
                needed: 1,
            });
        }
        Ok(Self {
            source,
            registers,
            location,
        })
    }

    /// Decode one context-selected decision and charge entropy work.
    pub fn decode_bit(
        &mut self,
        contexts: &mut ArithmeticContexts,
        index: usize,
        budget: &mut DecodeBudget,
    ) -> Result<bool> {
        budget.charge(LimitKind::Work, 1)?;
        let context = contexts
            .states
            .get_mut(index)
            .ok_or(Jbig2Error::InvalidCodingData {
                location: self.location,
                reason: "arithmetic context index",
            })?;
        self.registers
            .decode(&mut self.source, context)
            .map_err(|source| Jbig2Error::Arithmetic {
                location: self.location,
                source,
            })
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
