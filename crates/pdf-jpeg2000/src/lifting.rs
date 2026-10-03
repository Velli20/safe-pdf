//! The one-dimensional inverse wavelet filters.
//!
//! Annex F.3.8 reconstructs a signal from one interleaved array in which the
//! low-pass coefficients sit at even reference-grid positions and the
//! high-pass coefficients at odd ones. Which array index is even depends on
//! where the resolution level starts, so every filter takes that parity.
//!
//! Positions outside the array are the symmetric extension of Annex F.3.7:
//! the signal is mirrored about its first and last sample without repeating
//! either. Reading through [`reflect`] applies that extension in place, which
//! is correct because each lifting step reads only the parity it is not
//! writing, and the extension of a symmetric signal is symmetric again.

/// Samples below which a signal has no lifting to do.
const MINIMUM_LENGTH: usize = 2;

/// A sample type with an inverse wavelet filter.
///
/// Part 1 pairs the reversible 5/3 filter with integer samples and the
/// irreversible 9/7 filter with real ones, so the sample type determines the
/// filter.
pub(crate) trait Lifting: Copy + Default {
    /// Reconstructs one interleaved signal in place.
    ///
    /// `parity` is the reference-grid parity of the first sample: zero when
    /// the signal starts on a low-pass coefficient.
    fn inverse(signal: &mut [Self], parity: usize);
}

/// Returns the index the symmetric extension maps a position to.
///
/// Positions inside the signal map to themselves; positions outside are
/// mirrored about the first or last sample, repeatedly for a short signal.
pub(crate) fn reflect(position: isize, length: usize) -> usize {
    let Ok(limit) = isize::try_from(length) else {
        return 0;
    };
    if limit <= 1 {
        return 0;
    }
    let period = limit.saturating_sub(1).saturating_mul(2);
    let mut folded = position.rem_euclid(period);
    if folded > limit.saturating_sub(1) {
        folded = period.saturating_sub(folded);
    }
    usize::try_from(folded).unwrap_or(0)
}

/// Returns the sample at a possibly out-of-range position.
pub(crate) fn sample<T: Copy + Default>(signal: &[T], position: isize) -> T {
    signal
        .get(reflect(position, signal.len()))
        .copied()
        .unwrap_or_default()
}

/// Returns whether a signal is too short for the lifting steps to apply.
pub(crate) fn is_degenerate(signal: &[impl Sized]) -> bool {
    signal.len() < MINIMUM_LENGTH
}

/// Returns the positions of one parity within a signal.
pub(crate) fn positions(length: usize, parity: usize) -> impl Iterator<Item = usize> {
    (parity.min(length)..length).step_by(2)
}
