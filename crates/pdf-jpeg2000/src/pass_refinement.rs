//! The magnitude refinement pass.
//!
//! Annex D.3.3 adds one magnitude bit to every coefficient that was already
//! significant before this bit-plane began. Coefficients the significance
//! propagation pass just coded are marked visited and are skipped, because
//! their first magnitude bit is the one that made them significant.

use crate::{
    Jpeg2000Error,
    code_block_state::{CodeBlockState, CoefficientFlags},
    context_tables::refinement_context,
    mq_contexts::ContextSet,
    pass_coder::{PassScope, SegmentCoder, stripes},
};

/// Runs one magnitude refinement pass over a code-block.
///
/// # Errors
///
/// Returns a structural error when the codeword segment runs out of bits.
pub(crate) fn run(
    coder: &mut SegmentCoder<'_>,
    contexts: &mut ContextSet,
    state: &mut CodeBlockState,
    scope: PassScope,
) -> Result<(), Jpeg2000Error> {
    let size = state.size();
    for (stripe_start, stripe_end) in stripes(size.height) {
        for x in 0..size.width {
            for y in stripe_start..stripe_end {
                step(coder, contexts, state, scope, x, y, stripe_end)?;
            }
        }
    }
    Ok(())
}

/// Codes one coefficient of the magnitude refinement pass.
fn step(
    coder: &mut SegmentCoder<'_>,
    contexts: &mut ContextSet,
    state: &mut CodeBlockState,
    scope: PassScope,
    x: u32,
    y: u32,
    stripe_end: u32,
) -> Result<(), Jpeg2000Error> {
    let flags = state.flags(x, y);
    if !flags.contains(CoefficientFlags::SIGNIFICANT) || flags.contains(CoefficientFlags::VISITED) {
        return Ok(());
    }
    let first = !flags.contains(CoefficientFlags::REFINED);
    let neighbours = state.neighbourhood(x, y, scope.stripe_limit(stripe_end));
    let label = refinement_context(first, neighbours);
    if coder.bit(contexts, label)? {
        state.set_magnitude_bit(x, y, scope.plane)?;
    }
    state.insert(x, y, CoefficientFlags::REFINED)
}
