//! The reversible 5/3 inverse filter.
//!
//! Annex F.3.8.2 reconstructs the even, low-pass samples first and then the
//! odd, high-pass ones, using only additions and arithmetic shifts so the
//! result is exactly the encoder's input.

use crate::{
    coefficient::Coefficient,
    lifting::{Lifting, is_degenerate, positions, sample},
};

/// Rounding offset of the first lifting step.
const UPDATE_OFFSET: i64 = 2;
/// Shift of the first lifting step, which divides by four.
const UPDATE_SHIFT: u32 = 2;
/// Shift of the second lifting step, which divides by two.
const PREDICT_SHIFT: u32 = 1;

/// Applies the 5/3 inverse filter to an integer signal.
fn inverse<C: Coefficient>(signal: &mut [C], parity: usize) {
    if is_degenerate(signal) {
        // Annex F.3.7 halves a lone high-pass sample and leaves a lone
        // low-pass sample as it is.
        if parity == 1
            && let Some(only) = signal.first_mut()
        {
            *only = only.shift_right(1);
        }
        return;
    }
    for index in positions(signal.len(), parity) {
        let update = neighbour_sum(signal, index)
            .saturating_add(UPDATE_OFFSET)
            .div_euclid(1 << UPDATE_SHIFT);
        update_at(signal, index, update.checked_neg().unwrap_or(i64::MIN));
    }
    for index in positions(signal.len(), 1_usize.saturating_sub(parity)) {
        let predict = neighbour_sum(signal, index).div_euclid(1 << PREDICT_SHIFT);
        update_at(signal, index, predict);
    }
}

/// Returns the sum of a sample's two neighbours under symmetric extension.
fn neighbour_sum<C: Coefficient>(signal: &[C], index: usize) -> i64 {
    let position = isize::try_from(index).unwrap_or(0);
    let left = sample(signal, position.saturating_sub(1)).to_i64();
    let right = sample(signal, position.saturating_add(1)).to_i64();
    left.saturating_add(right)
}

/// Adds a lifting term to one sample.
fn update_at<C: Coefficient>(signal: &mut [C], index: usize, term: i64) {
    let Some(slot) = signal.get_mut(index) else {
        return;
    };
    let updated = slot.to_i64().saturating_add(term);
    *slot = C::from_i64(updated).unwrap_or(*slot);
}

impl Lifting for i32 {
    fn inverse(signal: &mut [Self], parity: usize) {
        inverse(signal, parity);
    }
}

impl Lifting for i64 {
    fn inverse(signal: &mut [Self], parity: usize) {
        inverse(signal, parity);
    }
}
