//! Coefficient magnitudes and per-coefficient flags of one code-block.
//!
//! Annex D decodes a code-block one bit-plane at a time, and every decision
//! depends on the eight neighbours of the coefficient being coded. The flag
//! plane therefore carries a one-coefficient border, so a neighbour read at
//! the edge of the block lands on a real, permanently insignificant cell
//! instead of needing a bounds test of its own.
//!
//! Magnitudes accumulate as unsigned integers with each bit at its true
//! position; the sign lives in the flags. Turning that into a signed,
//! dequantized coefficient is Annex E's work, not Annex D's.

use bitflags::bitflags;
use pdf_graphics::Size;

use crate::{Jpeg2000Error, workspace::Workspace};

/// Coefficients of border padding on each side of the flag plane.
const BORDER: u32 = 1;
/// Rows in one Annex D scan stripe.
pub(crate) const STRIPE_HEIGHT: u32 = 4;

bitflags! {
    /// Per-coefficient state maintained across the coding passes.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub(crate) struct CoefficientFlags: u8 {
        /// The coefficient has become significant in some bit-plane.
        const SIGNIFICANT = 1 << 0;
        /// The coefficient was coded by the current significance pass.
        const VISITED = 1 << 1;
        /// The coefficient's sign bit is negative.
        const NEGATIVE = 1 << 2;
        /// The coefficient has already received a magnitude refinement.
        const REFINED = 1 << 3;
    }
}

/// Significant neighbours of one coefficient, as Annex D groups them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Neighbourhood {
    /// Significant neighbours to the left and right.
    pub(crate) horizontal: u8,
    /// Significant neighbours above and below.
    pub(crate) vertical: u8,
    /// Significant neighbours on the four diagonals.
    pub(crate) diagonal: u8,
    /// Sum of the left and right signs, positive counting as one.
    pub(crate) horizontal_sign: i8,
    /// Sum of the upper and lower signs, positive counting as one.
    pub(crate) vertical_sign: i8,
}

impl Neighbourhood {
    /// Returns whether any of the eight neighbours is significant.
    pub(crate) fn any_significant(self) -> bool {
        self.horizontal | self.vertical | self.diagonal != 0
    }
}

/// The working state of one code-block while its bit-planes are decoded.
///
/// The buffers are sized once for the largest code-block a tile can hold and
/// reused for every block, so decoding a tile does not allocate per block.
#[derive(Debug)]
pub(crate) struct CodeBlockState {
    size: Size<u32>,
    stride: usize,
    capacity: Size<u32>,
    flags: Vec<CoefficientFlags>,
    magnitudes: Vec<u64>,
}

impl CodeBlockState {
    /// Allocates state for code-blocks up to `capacity` coefficients across.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` when the buffers pass the caller's
    /// working-memory bound, and `Overflow` if the capacity cannot be sized.
    pub(crate) fn new(
        capacity: Size<u32>,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "code-block state size",
        };
        let padded = Self::padded_cells(capacity).ok_or_else(overflow)?;
        let cells = usize::try_from(capacity.width)
            .ok()
            .and_then(|width| width.checked_mul(usize::try_from(capacity.height).ok()?))
            .ok_or_else(overflow)?;
        Ok(Self {
            size: Size {
                width: 0,
                height: 0,
            },
            stride: 0,
            capacity,
            flags: workspace.vector(padded)?,
            magnitudes: workspace.vector(cells)?,
        })
    }

    /// Clears the state and resizes it for the next code-block.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the block is larger than the allocated capacity.
    pub(crate) fn begin(&mut self, size: Size<u32>) -> Result<(), Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "code-block size",
        };
        if size.width > self.capacity.width || size.height > self.capacity.height {
            return Err(overflow());
        }
        let padded = Self::padded_cells(size).ok_or_else(overflow)?;
        let cells = usize::try_from(size.width)
            .ok()
            .and_then(|width| width.checked_mul(usize::try_from(size.height).ok()?))
            .ok_or_else(overflow)?;
        self.size = size;
        self.stride = usize::try_from(size.width.saturating_add(2)).map_err(|_| overflow())?;
        for slot in self.flags.get_mut(..padded).ok_or_else(overflow)? {
            *slot = CoefficientFlags::empty();
        }
        for slot in self.magnitudes.get_mut(..cells).ok_or_else(overflow)? {
            *slot = 0;
        }
        Ok(())
    }

    /// Returns the extent of the code-block being decoded.
    pub(crate) fn size(&self) -> Size<u32> {
        self.size
    }

    /// Returns the flags of one coefficient, or empty flags outside the block.
    pub(crate) fn flags(&self, x: u32, y: u32) -> CoefficientFlags {
        self.flag_index(x, y)
            .and_then(|index| self.flags.get(index).copied())
            .unwrap_or_else(CoefficientFlags::empty)
    }

    /// Adds flags to one coefficient.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the coefficient lies outside the code-block.
    pub(crate) fn insert(
        &mut self,
        x: u32,
        y: u32,
        flags: CoefficientFlags,
    ) -> Result<(), Jpeg2000Error> {
        let slot = self
            .flag_index(x, y)
            .and_then(|index| self.flags.get_mut(index))
            .ok_or(Jpeg2000Error::Overflow {
                context: "code-block coefficient index",
            })?;
        slot.insert(flags);
        Ok(())
    }

    /// Clears the visited flag on every coefficient of the block.
    pub(crate) fn clear_visited(&mut self) {
        for slot in &mut self.flags {
            slot.remove(CoefficientFlags::VISITED);
        }
    }

    /// Sets one bit of a coefficient's magnitude.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the coefficient lies outside the code-block or
    /// the bit-plane is outside the representable range.
    pub(crate) fn set_magnitude_bit(
        &mut self,
        x: u32,
        y: u32,
        plane: u32,
    ) -> Result<(), Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "code-block magnitude bit",
        };
        let bit = 1u64.checked_shl(plane).ok_or_else(overflow)?;
        let slot = self
            .magnitude_index(x, y)
            .and_then(|index| self.magnitudes.get_mut(index))
            .ok_or_else(overflow)?;
        *slot |= bit;
        Ok(())
    }

    /// Returns the magnitudes of the block in raster order.
    pub(crate) fn magnitudes(&self) -> &[u64] {
        let cells = usize::try_from(self.size.width)
            .ok()
            .and_then(|width| width.checked_mul(usize::try_from(self.size.height).ok()?))
            .unwrap_or(0);
        self.magnitudes.get(..cells).unwrap_or_default()
    }

    /// Returns whether a coefficient's decoded sign is negative.
    pub(crate) fn is_negative(&self, x: u32, y: u32) -> bool {
        self.flags(x, y).contains(CoefficientFlags::NEGATIVE)
    }

    /// Counts the significant neighbours of one coefficient.
    ///
    /// Rows at or past `stripe_end` are treated as insignificant, which is
    /// how the vertically causal context option of Annex D.7 hides the stripe
    /// that has not been decoded yet.
    pub(crate) fn neighbourhood(&self, x: u32, y: u32, stripe_end: Option<u32>) -> Neighbourhood {
        let hidden = |row: u32| stripe_end.is_some_and(|end| row >= end);
        let significance = |column: Option<u32>, row: Option<u32>| -> (u8, i8) {
            match (column, row) {
                (Some(column), Some(row)) if !hidden(row) => {
                    let flags = self.flags(column, row);
                    let significant = flags.contains(CoefficientFlags::SIGNIFICANT);
                    let sign: i8 = if flags.contains(CoefficientFlags::NEGATIVE) {
                        -1
                    } else {
                        1
                    };
                    (u8::from(significant), if significant { sign } else { 0 })
                }
                _ => (0, 0),
            }
        };
        let left = x.checked_sub(1);
        let right = Some(x.saturating_add(1)).filter(|value| *value < self.size.width);
        let above = y.checked_sub(1);
        let below = Some(y.saturating_add(1)).filter(|value| *value < self.size.height);
        let (west, west_sign) = significance(left, Some(y));
        let (east, east_sign) = significance(right, Some(y));
        let (north, north_sign) = significance(Some(x), above);
        let (south, south_sign) = significance(Some(x), below);
        let corners = [
            significance(left, above),
            significance(right, above),
            significance(left, below),
            significance(right, below),
        ];
        Neighbourhood {
            horizontal: west.saturating_add(east),
            vertical: north.saturating_add(south),
            diagonal: corners
                .iter()
                .fold(0u8, |total, (count, _)| total.saturating_add(*count)),
            horizontal_sign: west_sign.saturating_add(east_sign),
            vertical_sign: north_sign.saturating_add(south_sign),
        }
    }

    /// Returns the flag-plane index of a coefficient inside the block.
    fn flag_index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.size.width || y >= self.size.height {
            return None;
        }
        let column = usize::try_from(x.checked_add(BORDER)?).ok()?;
        let row = usize::try_from(y.checked_add(BORDER)?).ok()?;
        row.checked_mul(self.stride)?.checked_add(column)
    }

    /// Returns the magnitude index of a coefficient inside the block.
    fn magnitude_index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.size.width || y >= self.size.height {
            return None;
        }
        let column = usize::try_from(x).ok()?;
        let row = usize::try_from(y).ok()?;
        row.checked_mul(usize::try_from(self.size.width).ok()?)?
            .checked_add(column)
    }

    /// Returns the flag-plane cell count for a block of the given size.
    fn padded_cells(size: Size<u32>) -> Option<usize> {
        let padded =
            |extent: u32| usize::try_from(extent.checked_add(BORDER.checked_mul(2)?)?).ok();
        padded(size.width)?.checked_mul(padded(size.height)?)
    }
}
