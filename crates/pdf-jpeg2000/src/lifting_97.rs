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

use crate::lifting::{Lifting, is_degenerate, positions, sample};

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

impl Lifting for f64 {
    fn inverse(signal: &mut [Self], parity: usize) {
        if is_degenerate(signal) {
            return;
        }
        let low = parity;
        let high = 1_usize.saturating_sub(parity);
        scale(signal, low, SCALE);
        scale(signal, high, 1.0 / SCALE);
        lift(signal, low, DELTA);
        lift(signal, high, GAMMA);
        lift(signal, low, BETA);
        lift(signal, high, ALPHA);
    }
}

/// Multiplies every sample of one parity by a constant.
fn scale(signal: &mut [f64], parity: usize, factor: f64) {
    for index in positions(signal.len(), parity) {
        if let Some(slot) = signal.get_mut(index) {
            *slot *= factor;
        }
    }
}

/// Subtracts one lifting term from every sample of one parity.
fn lift(signal: &mut [f64], parity: usize, factor: f64) {
    for index in positions(signal.len(), parity) {
        let position = isize::try_from(index).unwrap_or(0);
        let left = sample(signal, position.saturating_sub(1));
        let right = sample(signal, position.saturating_add(1));
        if let Some(slot) = signal.get_mut(index) {
            *slot -= (left + right) * factor;
        }
    }
}
