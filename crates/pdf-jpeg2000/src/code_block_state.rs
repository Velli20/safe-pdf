//! Coefficient magnitudes and per-coefficient flags of one code-block.
//!
//! Annex D decodes a code-block one bit-plane at a time, and every decision
//! depends on the eight neighbours of the coefficient being coded. Each cell
//! of the flag plane therefore also records which of its neighbours are
//! significant and, for the four edge neighbours, their signs. A coefficient
//! updates its neighbours' cells once, when it becomes significant, so a
//! decision reads one cell instead of eight. The plane carries a
//! one-coefficient border, so the neighbours of an edge coefficient are real
//! cells that need no bounds test of their own.
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
    pub(crate) struct CoefficientFlags: u16 {
        /// The coefficient has become significant in some bit-plane.
        const SIGNIFICANT = 1 << 0;
        /// The coefficient was coded by the current significance pass.
        const VISITED = 1 << 1;
        /// The coefficient's sign bit is negative.
        const NEGATIVE = 1 << 2;
        /// The coefficient has already received a magnitude refinement.
        const REFINED = 1 << 3;
        /// The neighbour above is significant.
        const NORTH = 1 << 4;
        /// The neighbour below is significant.
        const SOUTH = 1 << 5;
        /// The neighbour to the left is significant.
        const WEST = 1 << 6;
        /// The neighbour to the right is significant.
        const EAST = 1 << 7;
        /// The neighbour above and to the left is significant.
        const NORTH_WEST = 1 << 8;
        /// The neighbour above and to the right is significant.
        const NORTH_EAST = 1 << 9;
        /// The neighbour below and to the left is significant.
        const SOUTH_WEST = 1 << 10;
        /// The neighbour below and to the right is significant.
        const SOUTH_EAST = 1 << 11;
        /// The neighbour above is negative.
        const NORTH_NEGATIVE = 1 << 12;
        /// The neighbour below is negative.
        const SOUTH_NEGATIVE = 1 << 13;
        /// The neighbour to the left is negative.
        const WEST_NEGATIVE = 1 << 14;
        /// The neighbour to the right is negative.
        const EAST_NEGATIVE = 1 << 15;
        /// Every significant neighbour.
        const NEIGHBOURS = Self::NORTH.bits()
            | Self::SOUTH.bits()
            | Self::WEST.bits()
            | Self::EAST.bits()
            | Self::NORTH_WEST.bits()
            | Self::NORTH_EAST.bits()
            | Self::SOUTH_WEST.bits()
            | Self::SOUTH_EAST.bits();
        /// Everything recorded about the row below, which the vertically
        /// causal context option hides at the bottom of a stripe.
        const BELOW = Self::SOUTH.bits()
            | Self::SOUTH_WEST.bits()
            | Self::SOUTH_EAST.bits()
            | Self::SOUTH_NEGATIVE.bits();
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

    /// Returns each row of the block as its magnitudes and the matching
    /// coefficient flags, which carry the signs.
    pub(crate) fn rows(&self) -> impl Iterator<Item = (&[u64], &[CoefficientFlags])> {
        let width = usize::try_from(self.size.width).unwrap_or(0).max(1);
        let padded_rows = self.flags.chunks_exact(self.stride.max(1)).skip(1);
        self.magnitudes()
            .chunks_exact(width)
            .zip(padded_rows)
            .map(|(magnitudes, flags)| (magnitudes, flags.get(1..).unwrap_or_default()))
    }

    /// Records that a coefficient became significant with the given sign.
    ///
    /// Each of the eight neighbouring cells learns about it, so its own
    /// neighbourhood can later be read from that one cell. Neighbours outside
    /// the block are border cells, which nothing reads as a coefficient.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the coefficient lies outside the code-block.
    pub(crate) fn make_significant(
        &mut self,
        x: u32,
        y: u32,
        negative: bool,
    ) -> Result<(), Jpeg2000Error> {
        let centre = self.flag_index(x, y).ok_or(Jpeg2000Error::Overflow {
            context: "code-block coefficient index",
        })?;
        let mut own = CoefficientFlags::SIGNIFICANT;
        own.set(CoefficientFlags::NEGATIVE, negative);
        let sign = |flag: CoefficientFlags| match negative {
            true => flag,
            false => CoefficientFlags::empty(),
        };
        let above = centre.checked_sub(self.stride);
        let below = centre.checked_add(self.stride);
        let updates = [
            (Some(centre), own),
            (
                above,
                CoefficientFlags::SOUTH | sign(CoefficientFlags::SOUTH_NEGATIVE),
            ),
            (
                below,
                CoefficientFlags::NORTH | sign(CoefficientFlags::NORTH_NEGATIVE),
            ),
            (
                centre.checked_sub(1),
                CoefficientFlags::EAST | sign(CoefficientFlags::EAST_NEGATIVE),
            ),
            (
                centre.checked_add(1),
                CoefficientFlags::WEST | sign(CoefficientFlags::WEST_NEGATIVE),
            ),
            (
                above.and_then(|row| row.checked_sub(1)),
                CoefficientFlags::SOUTH_EAST,
            ),
            (
                above.and_then(|row| row.checked_add(1)),
                CoefficientFlags::SOUTH_WEST,
            ),
            (
                below.and_then(|row| row.checked_sub(1)),
                CoefficientFlags::NORTH_EAST,
            ),
            (
                below.and_then(|row| row.checked_add(1)),
                CoefficientFlags::NORTH_WEST,
            ),
        ];
        for (index, flags) in updates {
            if let Some(slot) = index.and_then(|index| self.flags.get_mut(index)) {
                slot.insert(flags);
            }
        }
        Ok(())
    }

    /// Returns whether any neighbour of a coefficient is significant.
    ///
    /// This is the test for a zero significance context, without counting
    /// the neighbours; `stripe_end` hides rows as [`Self::neighbourhood`]
    /// describes.
    pub(crate) fn has_significant_neighbour(
        &self,
        x: u32,
        y: u32,
        stripe_end: Option<u32>,
    ) -> bool {
        self.visible_flags(x, y, stripe_end)
            .intersects(CoefficientFlags::NEIGHBOURS)
    }

    /// Returns a coefficient's flags with any hidden row below it removed.
    fn visible_flags(&self, x: u32, y: u32, stripe_end: Option<u32>) -> CoefficientFlags {
        let mut flags = self.flags(x, y);
        if stripe_end.is_some_and(|end| y.saturating_add(1) >= end) {
            flags.remove(CoefficientFlags::BELOW);
        }
        flags
    }

    /// Counts the significant neighbours of one coefficient.
    ///
    /// Rows at or past `stripe_end` are treated as insignificant, which is
    /// how the vertically causal context option of Annex D.7 hides the stripe
    /// that has not been decoded yet. Only the row below a coefficient can lie
    /// there.
    pub(crate) fn neighbourhood(&self, x: u32, y: u32, stripe_end: Option<u32>) -> Neighbourhood {
        let flags = self.visible_flags(x, y, stripe_end);
        let count = |mask: CoefficientFlags| {
            u8::try_from(flags.intersection(mask).bits().count_ones()).unwrap_or(u8::MAX)
        };
        let sign = |significant: CoefficientFlags, negative: CoefficientFlags| -> i8 {
            match (flags.contains(significant), flags.contains(negative)) {
                (false, _) => 0,
                (true, false) => 1,
                (true, true) => -1,
            }
        };
        Neighbourhood {
            horizontal: count(CoefficientFlags::WEST | CoefficientFlags::EAST),
            vertical: count(CoefficientFlags::NORTH | CoefficientFlags::SOUTH),
            diagonal: count(
                CoefficientFlags::NORTH_WEST
                    | CoefficientFlags::NORTH_EAST
                    | CoefficientFlags::SOUTH_WEST
                    | CoefficientFlags::SOUTH_EAST,
            ),
            horizontal_sign: sign(CoefficientFlags::WEST, CoefficientFlags::WEST_NEGATIVE)
                .saturating_add(sign(
                    CoefficientFlags::EAST,
                    CoefficientFlags::EAST_NEGATIVE,
                )),
            vertical_sign: sign(CoefficientFlags::NORTH, CoefficientFlags::NORTH_NEGATIVE)
                .saturating_add(sign(
                    CoefficientFlags::SOUTH,
                    CoefficientFlags::SOUTH_NEGATIVE,
                )),
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
