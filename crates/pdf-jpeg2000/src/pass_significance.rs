//! The significance propagation pass.
//!
//! Annex D.3.1 visits, in stripe scan order, every coefficient that is not yet
//! significant but has at least one significant neighbour, and codes whether
//! this bit-plane makes it significant. A coefficient the pass codes is marked
//! as visited, so the refinement and cleanup passes of the same bit-plane know
//! to leave it alone.

use crate::{
    Jpeg2000Error,
    code_block_state::{CodeBlockState, CoefficientFlags},
    context_tables::significance_context,
    mq_contexts::ContextSet,
    pass_coder::{PassScope, SegmentCoder, decode_sign, stripes},
};

/// Runs one significance propagation pass over a code-block.
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

/// Codes one coefficient of the significance propagation pass.
fn step(
    coder: &mut SegmentCoder<'_>,
    contexts: &mut ContextSet,
    state: &mut CodeBlockState,
    scope: PassScope,
    x: u32,
    y: u32,
    stripe_end: u32,
) -> Result<(), Jpeg2000Error> {
    if state.flags(x, y).contains(CoefficientFlags::SIGNIFICANT) {
        return Ok(());
    }
    let neighbours = state.neighbourhood(x, y, scope.stripe_limit(stripe_end));
    let label = significance_context(scope.band, neighbours);
    if label.is_insignificant() {
        return Ok(());
    }
    if coder.bit(contexts, label)? {
        state.set_magnitude_bit(x, y, scope.plane)?;
        state.insert(x, y, CoefficientFlags::SIGNIFICANT)?;
        decode_sign(coder, contexts, state, neighbours, x, y)?;
    }
    state.insert(x, y, CoefficientFlags::VISITED)
}
