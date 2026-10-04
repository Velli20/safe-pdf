//! The one-dimensional inverse wavelet filters.
//!
//! Annex F.3.8 reconstructs a signal from one interleaved array in which the
//! low-pass coefficients sit at even reference-grid positions and the
//! high-pass coefficients at odd ones. Which array index is even depends on
//! where the resolution level starts, so every filter takes that parity.
//!
//! Positions outside the array are the symmetric extension of Annex F.3.7:
//! the signal is mirrored about its first and last sample without repeating
//! either. Each lifting step reads only the parity it is not writing, so the
//! only positions it ever reads outside the signal are one before the first
//! sample and one past the last, which mirror to the second sample and the
//! one before the last.
//!
//! A filter runs over lanes: `lanes` independent signals stored sample by
//! sample, so that position `k` of every lane is one contiguous run. A row is
//! a single lane; a strip of columns gathered row by row is many, which lets
//! the vertical pass lift whole runs at once instead of striding down the
//! coefficient plane one column at a time.

/// Samples below which a signal has no lifting to do.
const MINIMUM_LENGTH: usize = 2;

/// A sample type with an inverse wavelet filter.
///
/// Part 1 pairs the reversible 5/3 filter with integer samples and the
/// irreversible 9/7 filter with real ones, so the sample type determines the
/// filter. A filter is a rescaling of each parity followed by lifting steps
/// that alternate between the low-pass and the high-pass parity, starting
/// with the low-pass one.
pub(crate) trait Lifting: Copy + Default {
    /// What one lifting step needs to know, resolved once per step.
    type Step: Copy;

    /// Lifting steps of the inverse filter.
    const STEPS: usize;

    /// Returns lifting step `index` of the inverse filter.
    fn step(index: usize) -> Self::Step;

    /// Undoes the gain the forward transform applied to one parity.
    fn rescale(self, low: bool) -> Self;

    /// Applies one lifting step to a sample from its two neighbours.
    fn lift(step: Self::Step, sample: Self, left: Self, right: Self) -> Self;

    /// Reconstructs a signal that consists of this one sample.
    fn single(self, low: bool) -> Self;
}

/// Reconstructs `lanes` interleaved signals in place.
///
/// `data` holds the signals position by position, `lanes` samples per
/// position. `parity` is the reference-grid parity of the first position:
/// zero when the signals start on a low-pass coefficient.
pub(crate) fn inverse<T: Lifting>(data: &mut [T], lanes: usize, parity: usize) {
    let length = data.len().checked_div(lanes).unwrap_or(0);
    if length < MINIMUM_LENGTH {
        for sample in data {
            *sample = sample.single(parity == 0);
        }
        return;
    }
    if lanes == 1 {
        return inverse_signal(data, parity);
    }
    for (position, run) in data.chunks_exact_mut(lanes).enumerate() {
        let low = position % 2 == parity;
        for sample in run {
            *sample = sample.rescale(low);
        }
    }
    let high = 1_usize.saturating_sub(parity);
    for index in 0..T::STEPS {
        let target = if index % 2 == 0 { parity } else { high };
        let step = T::step(index);
        let mut position = target;
        while position < length {
            let (left, right) = neighbours(position, length);
            lift_run(data, lanes, position, left, right, step);
            position = position.saturating_add(2);
        }
    }
}

/// Reconstructs one signal of at least two samples in place.
fn inverse_signal<T: Lifting>(signal: &mut [T], parity: usize) {
    let high = 1_usize.saturating_sub(parity);
    for (position, sample) in signal.iter_mut().enumerate() {
        *sample = sample.rescale(position % 2 == parity);
    }
    for index in 0..T::STEPS {
        let target = if index % 2 == 0 { parity } else { high };
        let step = T::step(index);
        let mut position = target;
        while position < signal.len() {
            let (left, right) = neighbours(position, signal.len());
            if let (Some(left), Some(right)) =
                (signal.get(left).copied(), signal.get(right).copied())
                && let Some(sample) = signal.get_mut(position)
            {
                *sample = T::lift(step, *sample, left, right);
            }
            position = position.saturating_add(2);
        }
    }
}

/// Returns the positions a lifting step reads around `position`.
///
/// The symmetric extension mirrors position -1 to 1 and `length` to
/// `length - 2`; both have the other parity, like every neighbour.
fn neighbours(position: usize, length: usize) -> (usize, usize) {
    let last = length.saturating_sub(1);
    let left = position.checked_sub(1).unwrap_or(1);
    let right = if position < last {
        position.saturating_add(1)
    } else {
        last.saturating_sub(1)
    };
    (left, right)
}

/// Lifts the run at one position from the runs at two other positions.
fn lift_run<T: Lifting>(
    data: &mut [T],
    lanes: usize,
    position: usize,
    left: usize,
    right: usize,
    step: T::Step,
) {
    let Some((before, rest)) = data.split_at_mut_checked(position.saturating_mul(lanes)) else {
        return;
    };
    let Some((target, after)) = rest.split_at_mut_checked(lanes) else {
        return;
    };
    let (Some(left), Some(right)) = (
        neighbour(before, after, lanes, position, left),
        neighbour(before, after, lanes, position, right),
    ) else {
        return;
    };
    for ((sample, left), right) in target.iter_mut().zip(left).zip(right) {
        *sample = T::lift(step, *sample, *left, *right);
    }
}

/// Returns the run at `other`, which lies before or after `position`.
fn neighbour<'a, T>(
    before: &'a [T],
    after: &'a [T],
    lanes: usize,
    position: usize,
    other: usize,
) -> Option<&'a [T]> {
    let (runs, index) = match other.checked_sub(position) {
        Some(distance) => (after, distance.checked_sub(1)?),
        None => (before, other),
    };
    let start = index.checked_mul(lanes)?;
    runs.get(start..start.checked_add(lanes)?)
}
