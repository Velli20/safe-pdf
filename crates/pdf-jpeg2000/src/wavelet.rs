//! The two-dimensional inverse wavelet transform of one tile-component.
//!
//! Annex F.3.2 reconstructs a tile-component by repeating a single step: take
//! the samples of resolution level `r - 1` together with the three subbands
//! of level `r`, interleave them, and filter the result along each axis to
//! obtain resolution level `r`.
//!
//! The coefficients live in one buffer the size of the tile-component, laid
//! out so that each level's subbands sit to the right of and below the level
//! beneath them. That is the layout inverse quantization writes, and it lets
//! every step read and write the same buffer.

use crate::{Jpeg2000Error, lifting::Lifting, region::Region};

/// Reconstructs one tile-component from its subband coefficients.
///
/// `resolutions` lists the region of every level from zero to the component's
/// decomposition count, and `plane` holds the coefficients in the layout
/// described above with `stride` samples between rows.
///
/// # Errors
///
/// Returns `Overflow` if a level's geometry does not fit the buffer.
pub(crate) fn reconstruct<T: Lifting>(
    plane: &mut [T],
    stride: usize,
    resolutions: &[Region],
    scratch: &mut [T],
) -> Result<(), Jpeg2000Error> {
    for level in 1..resolutions.len() {
        let (Some(previous), Some(current)) = (
            resolutions.get(level.saturating_sub(1)),
            resolutions.get(level),
        ) else {
            continue;
        };
        let step = Step {
            previous: *previous,
            current: *current,
            stride,
        };
        step.rows(plane, scratch)?;
        step.columns(plane, scratch)?;
    }
    Ok(())
}

/// One resolution level's reconstruction from the level beneath it.
#[derive(Clone, Copy, Debug)]
struct Step {
    previous: Region,
    current: Region,
    stride: usize,
}

impl Step {
    /// Filters every row of the level along the horizontal axis.
    fn rows<T: Lifting>(self, plane: &mut [T], scratch: &mut [T]) -> Result<(), Jpeg2000Error> {
        let width = self.length(self.current.size().width)?;
        let low = self.length(self.previous.size().width)?;
        let parity = usize::try_from(self.current.origin().x % 2).unwrap_or(0);
        for row in 0..self.length(self.current.size().height)? {
            let start = row.checked_mul(self.stride).ok_or_else(Self::overflow)?;
            let end = start.checked_add(width).ok_or_else(Self::overflow)?;
            let line = plane.get_mut(start..end).ok_or_else(Self::overflow)?;
            let signal = scratch.get_mut(..width).ok_or_else(Self::overflow)?;
            interleave(line, signal, low, parity);
            T::inverse(signal, parity);
            line.copy_from_slice(signal);
        }
        Ok(())
    }

    /// Filters every column of the level along the vertical axis.
    fn columns<T: Lifting>(self, plane: &mut [T], scratch: &mut [T]) -> Result<(), Jpeg2000Error> {
        let width = self.length(self.current.size().width)?;
        let height = self.length(self.current.size().height)?;
        let low = self.length(self.previous.size().height)?;
        let parity = usize::try_from(self.current.origin().y % 2).unwrap_or(0);
        let high = 1_usize.saturating_sub(parity);
        for index in 0..width {
            let signal = scratch.get_mut(..height).ok_or_else(Self::overflow)?;
            // Gathering and interleaving are one step for a column: row `k`
            // of the level's low half lands on the parity the level starts
            // on, and the high half on the other parity.
            for row in 0..height {
                let value =
                    *Self::cell(plane, self.stride, index, row).ok_or_else(Self::overflow)?;
                let target = if row < low {
                    parity.saturating_add(row.saturating_mul(2))
                } else {
                    high.saturating_add(row.saturating_sub(low).saturating_mul(2))
                };
                *signal.get_mut(target).ok_or_else(Self::overflow)? = value;
            }
            T::inverse(signal, parity);
            for row in 0..height {
                let value = *signal.get(row).ok_or_else(Self::overflow)?;
                *Self::cell_mut(plane, self.stride, index, row).ok_or_else(Self::overflow)? = value;
            }
        }
        Ok(())
    }

    /// Returns one cell of the coefficient plane.
    fn cell<T>(plane: &[T], stride: usize, column: usize, row: usize) -> Option<&T> {
        plane.get(row.checked_mul(stride)?.checked_add(column)?)
    }

    /// Returns one cell of the coefficient plane for writing.
    fn cell_mut<T>(plane: &mut [T], stride: usize, column: usize, row: usize) -> Option<&mut T> {
        plane.get_mut(row.checked_mul(stride)?.checked_add(column)?)
    }

    /// Converts an extent to a buffer length.
    fn length(self, extent: u32) -> Result<usize, Jpeg2000Error> {
        usize::try_from(extent).map_err(|_| Self::overflow())
    }

    /// Returns the error for geometry that does not fit the buffer.
    fn overflow() -> Jpeg2000Error {
        Jpeg2000Error::Overflow {
            context: "inverse wavelet geometry",
        }
    }
}

/// Interleaves a level's low-pass and high-pass halves into one signal.
///
/// Annex F.3.5 places the low-pass coefficients at even reference-grid
/// positions and the high-pass ones at odd positions, which in buffer terms
/// means the parity the level starts on and the other parity.
fn interleave<T: Copy>(source: &[T], signal: &mut [T], low: usize, parity: usize) {
    for (index, value) in source.iter().take(low).enumerate() {
        if let Some(slot) = signal.get_mut(parity.saturating_add(index.saturating_mul(2))) {
            *slot = *value;
        }
    }
    let high = 1_usize.saturating_sub(parity);
    for (index, value) in source.iter().skip(low).enumerate() {
        if let Some(slot) = signal.get_mut(high.saturating_add(index.saturating_mul(2))) {
            *slot = *value;
        }
    }
}
