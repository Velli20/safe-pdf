//! The entropy source and shared steps of the three coding passes.
//!
//! Annex D reads most decisions through the MQ coder, but the selective
//! arithmetic bypass of Annex D.7 makes the significance and refinement
//! passes of the higher bit-planes read raw bits instead. Both appear here as
//! one [`SegmentCoder`], because the passes differ only in which decisions
//! they make, not in where the bits come from.

use pdf_mq_coder::MqRegisters;

use crate::{
    Jpeg2000Error,
    chunk_source::ChainSource,
    code_block_state::{CodeBlockState, CoefficientFlags, Neighbourhood, STRIPE_HEIGHT},
    context_tables::sign_context,
    mq_contexts::{ContextLabel, ContextSet},
    resolution::BandKind,
    stuffed_bits::StuffedBitReader,
};

/// Returns the Annex D scan stripes of a code-block, as `(start, end)` rows.
///
/// Stripes are four rows tall and anchored at the top of the code-block, so
/// only the last one can be shorter.
pub(crate) fn stripes(height: u32) -> impl Iterator<Item = (u32, u32)> {
    let mut start = 0u32;
    core::iter::from_fn(move || {
        if start >= height {
            return None;
        }
        let stripe = (start, start.saturating_add(STRIPE_HEIGHT).min(height));
        start = stripe.1;
        Some(stripe)
    })
}

/// What a coding pass needs to know beyond the coefficients themselves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PassScope {
    /// Orientation of the subband the code-block belongs to.
    pub(crate) band: BandKind,
    /// Magnitude bit-plane the pass is decoding.
    pub(crate) plane: u32,
    /// Whether the vertically causal context option is in force.
    pub(crate) causal: bool,
}

impl PassScope {
    /// Returns the row past which neighbours are hidden, if any.
    ///
    /// The vertically causal option of Annex D.7 makes a stripe independent of
    /// the one below it by treating that stripe as insignificant.
    pub(crate) fn stripe_limit(self, stripe_end: u32) -> Option<u32> {
        self.causal.then_some(stripe_end)
    }
}

/// The bit supply of one codeword segment.
#[derive(Debug)]
pub(crate) enum SegmentCoder<'a> {
    /// Decisions come from the MQ arithmetic coder.
    Arithmetic {
        /// Interval and code registers of the segment's arithmetic run.
        registers: MqRegisters,
        /// Compressed bytes of the segment.
        source: ChainSource<'a>,
    },
    /// Decisions are raw bits, as the selective bypass produces.
    Raw(StuffedBitReader<ChainSource<'a>>),
}

impl<'a> SegmentCoder<'a> {
    /// Starts an arithmetic run over one codeword segment.
    pub(crate) fn arithmetic(chunks: &'a [&'a [u8]]) -> Self {
        let mut source = ChainSource::new(chunks);
        let registers = MqRegisters::new(&mut source);
        Self::Arithmetic { registers, source }
    }

    /// Starts a raw run over one bypassed codeword segment.
    pub(crate) fn raw(chunks: &'a [&'a [u8]], offset: usize) -> Self {
        Self::Raw(StuffedBitReader::new(ChainSource::new(chunks), offset))
    }

    /// Decodes one decision, adapting the context in arithmetic mode.
    ///
    /// # Errors
    ///
    /// Returns a structural error when the segment runs out of bits.
    pub(crate) fn bit(
        &mut self,
        contexts: &mut ContextSet,
        label: ContextLabel,
    ) -> Result<bool, Jpeg2000Error> {
        match self {
            Self::Arithmetic { registers, source } => {
                let context = contexts.context(label).ok_or(Jpeg2000Error::Overflow {
                    context: "arithmetic context label",
                })?;
                Ok(registers.decode(source, context)?)
            }
            Self::Raw(reader) => reader.read_bit(),
        }
    }

    /// Decodes a fixed-width value through the uniform context.
    ///
    /// # Errors
    ///
    /// Returns a structural error when the segment runs out of bits.
    pub(crate) fn uniform_bits(
        &mut self,
        contexts: &mut ContextSet,
        count: u32,
    ) -> Result<u32, Jpeg2000Error> {
        let mut value = 0u32;
        for _ in 0..count {
            let bit = self.bit(contexts, ContextLabel::UNIFORM)?;
            value = value.wrapping_shl(1) | u32::from(bit);
        }
        Ok(value)
    }
}

/// Decodes a coefficient's sign and records it on the code-block.
///
/// Annex D.3.2 codes the sign against a context chosen from the signs of the
/// four edge neighbours, together with a bit that flips the result. A raw
/// segment carries the sign directly, with neither context nor flip.
///
/// # Errors
///
/// Returns a structural error when the segment runs out of bits, or when the
/// coefficient lies outside the code-block.
pub(crate) fn decode_sign(
    coder: &mut SegmentCoder<'_>,
    contexts: &mut ContextSet,
    state: &mut CodeBlockState,
    neighbours: Neighbourhood,
    x: u32,
    y: u32,
) -> Result<(), Jpeg2000Error> {
    let (label, flip) = sign_context(neighbours);
    let negative = match coder {
        SegmentCoder::Arithmetic { .. } => coder.bit(contexts, label)? ^ flip,
        SegmentCoder::Raw(reader) => reader.read_bit()?,
    };
    if negative {
        state.insert(x, y, CoefficientFlags::NEGATIVE)?;
    }
    Ok(())
}
