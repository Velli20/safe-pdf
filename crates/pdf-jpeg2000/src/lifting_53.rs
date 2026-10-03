//! The reversible 5/3 inverse filter.
//!
//! Annex F.3.8.2 reconstructs the even, low-pass samples first and then the
//! odd, high-pass ones, using only additions and arithmetic shifts so the
//! result is exactly the encoder's input.

use crate::{coefficient::Coefficient, lifting::Lifting};

/// Rounding offset of the first lifting step.
const UPDATE_OFFSET: i64 = 2;
/// Shift of the first lifting step, which divides by four.
const UPDATE_SHIFT: u32 = 2;
/// Shift of the second lifting step, which divides by two.
const PREDICT_SHIFT: u32 = 1;
/// Lifting steps of the filter: the low-pass update, then the prediction.
const STEPS: [Step; 2] = [Step::Update, Step::Predict];

/// One of the two lifting steps of the 5/3 filter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Step {
    /// The first step, which updates the low-pass samples.
    Update,
    /// The second step, which predicts the high-pass samples.
    Predict,
}

/// Applies one 5/3 lifting step to an integer sample.
fn lift<C: Coefficient>(step: Step, sample: C, left: C, right: C) -> C {
    let sum = left.to_i64().saturating_add(right.to_i64());
    let term = match step {
        Step::Update => sum
            .saturating_add(UPDATE_OFFSET)
            .div_euclid(1 << UPDATE_SHIFT)
            .checked_neg()
            .unwrap_or(i64::MIN),
        Step::Predict => sum.div_euclid(1 << PREDICT_SHIFT),
    };
    C::from_i64(sample.to_i64().saturating_add(term)).unwrap_or(sample)
}

/// Reconstructs a one-sample signal.
///
/// Annex F.3.7 halves a lone high-pass sample and leaves a lone low-pass
/// sample as it is.
fn single<C: Coefficient>(sample: C, low: bool) -> C {
    match low {
        true => sample,
        false => sample.shift_right(1),
    }
}

impl Lifting for i32 {
    type Step = Step;

    const STEPS: usize = STEPS.len();

    fn step(index: usize) -> Step {
        STEPS.get(index).copied().unwrap_or(Step::Predict)
    }

    fn rescale(self, _low: bool) -> Self {
        self
    }

    fn lift(step: Step, sample: Self, left: Self, right: Self) -> Self {
        lift(step, sample, left, right)
    }

    fn single(self, low: bool) -> Self {
        single(self, low)
    }
}

impl Lifting for i64 {
    type Step = Step;

    const STEPS: usize = STEPS.len();

    fn step(index: usize) -> Step {
        STEPS.get(index).copied().unwrap_or(Step::Predict)
    }

    fn rescale(self, _low: bool) -> Self {
        self
    }

    fn lift(step: Step, sample: Self, left: Self, right: Self) -> Self {
        lift(step, sample, left, right)
    }

    fn single(self, low: bool) -> Self {
        single(self, low)
    }
}
