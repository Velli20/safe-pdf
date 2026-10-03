//! The irreversible 9/7 inverse filter.
//!
//! Annex F.3.8.2 undoes the four lifting steps of the forward transform in
//! reverse order, after rescaling each parity by the gain the forward
//! transform applied. The lifting parameters are Table F.4's, and the signs
//! are the standard's: `alpha` and `beta` are negative, so subtracting them
//! adds.
//!
//! The filter runs in `f64`. Annex F does not fix a precision, but the
//! lifting steps of every decomposition level compound, and single precision
//! loses enough over a deep decomposition to push the worst sample outside
//! the comparison criteria T.803 applies to irreversible decoding.

use crate::lifting::Lifting;

/// Table F.4 lifting parameter used by the last inverse step.
const ALPHA: f64 = -1.586_134_342_059_924;
/// Table F.4 lifting parameter used by the third inverse step.
const BETA: f64 = -0.052_980_118_572_961;
/// Table F.4 lifting parameter used by the second inverse step.
const GAMMA: f64 = 0.882_911_075_530_934;
/// Table F.4 lifting parameter used by the first inverse step.
const DELTA: f64 = 0.443_506_852_043_971;
/// Table F.4 scaling parameter of the forward transform.
const SCALE: f64 = 1.230_174_104_914_001;
/// Lifting parameters in the order the inverse filter applies them.
const STEPS: [f64; 4] = [DELTA, GAMMA, BETA, ALPHA];

impl Lifting for f64 {
    type Step = f64;

    const STEPS: usize = STEPS.len();

    fn step(index: usize) -> f64 {
        STEPS.get(index).copied().unwrap_or(0.0)
    }

    fn rescale(self, low: bool) -> Self {
        match low {
            true => self * SCALE,
            false => self * (1.0 / SCALE),
        }
    }

    fn lift(factor: f64, sample: Self, left: Self, right: Self) -> Self {
        sample - (left + right) * factor
    }

    fn single(self, _low: bool) -> Self {
        self
    }
}
