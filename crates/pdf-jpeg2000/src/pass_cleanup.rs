//! The cleanup pass.
//!
//! Annex D.3.4 codes every coefficient the other two passes of this bit-plane
//! left alone. Where a whole stripe column is insignificant and has no
//! significant neighbour at all — the common case in a sparse bit-plane — the
//! four coefficients are coded together through the run-length context, which
//! is what makes the cleanup pass cheap.
//!
//! The pass ends a bit-plane, so it clears the visited marks and, when the
//! code-block asks for it, checks the Annex D.5 segmentation symbol.

use crate::{
    Jpeg2000Error,
    code_block_state::{CodeBlockState, CoefficientFlags, STRIPE_HEIGHT},
    context_tables::significance_context,
    mq_contexts::{ContextLabel, ContextSet},
    pass_coder::{PassScope, SegmentCoder, decode_significant, stripes},
};

/// Bits of the run-length position, which names one of four rows.
const RUN_POSITION_BITS: u32 = 2;
/// Bits of the Annex D.5 segmentation symbol.
const SEGMENTATION_BITS: u32 = 4;
/// Value every segmentation symbol must take.
const SEGMENTATION_SYMBOL: u32 = 0b1010;

/// Runs one cleanup pass over a code-block.
///
/// # Errors
///
/// Returns a structural error when the codeword segment runs out of bits, or
/// when a segmentation symbol is wrong, which means the segment is corrupt.
pub(crate) fn run(
    coder: &mut SegmentCoder<'_>,
    contexts: &mut ContextSet,
    state: &mut CodeBlockState,
    scope: PassScope,
    segmentation: bool,
) -> Result<(), Jpeg2000Error> {
    let size = state.size();
    for (stripe_start, stripe_end) in stripes(size.height) {
        for x in 0..size.width {
            let first = run_length(coder, contexts, state, scope, x, stripe_start, stripe_end)?;
            for y in first..stripe_end {
                step(coder, contexts, state, scope, x, y, stripe_end)?;
            }
        }
    }
    state.clear_visited();
    if segmentation {
        check_segmentation(coder, contexts)?;
    }
    Ok(())
}

/// Codes a whole stripe column at once where Annex D.3.4 allows it.
///
/// Returns the first row the ordinary per-coefficient loop must still code,
/// which is the end of the stripe when the run-length decision reported that
/// every coefficient stays insignificant.
fn run_length(
    coder: &mut SegmentCoder<'_>,
    contexts: &mut ContextSet,
    state: &mut CodeBlockState,
    scope: PassScope,
    x: u32,
    stripe_start: u32,
    stripe_end: u32,
) -> Result<u32, Jpeg2000Error> {
    if !is_run_length_column(state, scope, x, stripe_start, stripe_end) {
        return Ok(stripe_start);
    }
    if !coder.bit(contexts, ContextLabel::RUN_LENGTH)? {
        return Ok(stripe_end);
    }
    let position = coder.uniform_bits(contexts, RUN_POSITION_BITS)?;
    let y = stripe_start.saturating_add(position);
    if y >= stripe_end {
        return Err(Jpeg2000Error::Overflow {
            context: "cleanup run-length position",
        });
    }
    let neighbours = state.neighbourhood(x, y, scope.stripe_limit(stripe_end));
    decode_significant(coder, contexts, state, neighbours, scope, x, y)?;
    Ok(y.saturating_add(1))
}

/// Returns whether a stripe column qualifies for run-length coding.
///
/// Annex D.3.4 requires a full four-row stripe in which no coefficient is
/// significant, none was coded by an earlier pass of this bit-plane, and none
/// has a significant neighbour, which is a zero significance context.
fn is_run_length_column(
    state: &CodeBlockState,
    scope: PassScope,
    x: u32,
    stripe_start: u32,
    stripe_end: u32,
) -> bool {
    if stripe_end.saturating_sub(stripe_start) != STRIPE_HEIGHT {
        return false;
    }
    (stripe_start..stripe_end).all(|y| {
        let flags = state.flags(x, y);
        !flags.intersects(CoefficientFlags::SIGNIFICANT | CoefficientFlags::VISITED)
            && !state.has_significant_neighbour(x, y, scope.stripe_limit(stripe_end))
    })
}

/// Codes one coefficient the earlier passes of this bit-plane skipped.
fn step(
    coder: &mut SegmentCoder<'_>,
    contexts: &mut ContextSet,
    state: &mut CodeBlockState,
    scope: PassScope,
    x: u32,
    y: u32,
    stripe_end: u32,
) -> Result<(), Jpeg2000Error> {
    if state
        .flags(x, y)
        .intersects(CoefficientFlags::SIGNIFICANT | CoefficientFlags::VISITED)
    {
        return Ok(());
    }
    let neighbours = state.neighbourhood(x, y, scope.stripe_limit(stripe_end));
    let label = significance_context(scope.band, neighbours);
    if coder.bit(contexts, label)? {
        decode_significant(coder, contexts, state, neighbours, scope, x, y)?;
    }
    Ok(())
}

/// Reads and checks the Annex D.5 segmentation symbol.
fn check_segmentation(
    coder: &mut SegmentCoder<'_>,
    contexts: &mut ContextSet,
) -> Result<(), Jpeg2000Error> {
    if coder.uniform_bits(contexts, SEGMENTATION_BITS)? == SEGMENTATION_SYMBOL {
        return Ok(());
    }
    Err(Jpeg2000Error::InvalidTilePart {
        offset: 0,
        tile: 0,
        part: 0,
        reason: "code-block segmentation symbol is wrong",
    })
}
