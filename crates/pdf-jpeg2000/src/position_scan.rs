//! Reference-grid stepping shared by the positional progression orders.
//!
//! `RPCL`, `PCRL`, and `CPRL` walk the tile's reference grid rather than a
//! precinct index, emitting a packet whenever a position coincides with the
//! upper-left corner of a precinct of the component and resolution being
//! visited (Annex B.12.1.3 to B.12.1.5). The stride of that walk is the
//! smallest precinct projection in scope, so no precinct corner is skipped.
//!
//! Every quantity here is evaluated in `u64`: a precinct exponent of 15 above
//! a 32-level decomposition shifts well past the width of the coordinates it
//! is applied to.

use pdf_graphics::{Size, point::Point};

use crate::{Jpeg2000Error, progression::ComponentProgression, region::Region};

/// Projection of one resolution level's precinct grid onto the tile grid.
///
/// The projection answers two questions for a reference-grid position: does a
/// precinct of this component and resolution start here, and if so, which one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PrecinctProjection {
    /// Reference-grid distance between neighbouring precinct corners.
    stride: Size<u64>,
    /// Sampled-grid distance between neighbouring precinct corners.
    sampling: Size<u64>,
    /// Precinct exponents on the resolution grid.
    exponents: Size<u32>,
    /// Upper-left corner of the resolution level on its own grid.
    origin: Point<u32>,
    /// Precinct columns in the resolution level.
    columns: u32,
    /// Whether the resolution level covers any sample at all.
    populated: bool,
}

impl PrecinctProjection {
    /// Projects resolution level `resolution` of one component onto the tile.
    ///
    /// Returns `None` when the component has no such resolution level.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the projection cannot be represented.
    pub(crate) fn new(
        component: &ComponentProgression,
        resolution: u8,
    ) -> Result<Option<Self>, Jpeg2000Error> {
        let Some(level) = component.resolution(resolution) else {
            return Ok(None);
        };
        let remaining = u32::from(component.levels.saturating_sub(resolution));
        let exponents = Size {
            width: u32::from(level.exponents.width),
            height: u32::from(level.exponents.height),
        };
        let sampling = Size {
            width: shifted(u64::from(component.subsampling.width), remaining)?,
            height: shifted(u64::from(component.subsampling.height), remaining)?,
        };
        Ok(Some(Self {
            stride: Size {
                width: shifted(sampling.width, exponents.width)?,
                height: shifted(sampling.height, exponents.height)?,
            },
            sampling,
            exponents,
            origin: level.region.origin(),
            columns: level.columns,
            populated: !level.region.is_empty() && level.columns > 0 && level.rows > 0,
        }))
    }

    /// Returns the reference-grid stride between precinct corners.
    pub(crate) fn stride(self) -> Size<u64> {
        self.stride
    }

    /// Returns whether the level contributes packets at all.
    pub(crate) fn populated(self) -> bool {
        self.populated
    }

    /// Returns the precinct starting at `position`, if one starts there.
    ///
    /// A precinct also starts at the tile origin when the resolution level
    /// begins part-way into a precinct, which is the second clause of the
    /// Annex B.12.1.3 test.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the precinct index cannot be represented.
    pub(crate) fn precinct_at(
        self,
        tile: Region,
        position: Point<u32>,
    ) -> Result<Option<u32>, Jpeg2000Error> {
        if !self.populated {
            return Ok(None);
        }
        let origin = tile.origin();
        if !self.starts(
            position.x,
            origin.x,
            self.stride.width,
            self.origin.x,
            self.exponents.width,
        ) || !self.starts(
            position.y,
            origin.y,
            self.stride.height,
            self.origin.y,
            self.exponents.height,
        ) {
            return Ok(None);
        }
        let column = self.cell(
            position.x,
            self.sampling.width,
            self.exponents.width,
            self.origin.x,
        )?;
        let row = self.cell(
            position.y,
            self.sampling.height,
            self.exponents.height,
            self.origin.y,
        )?;
        let index = u64::from(row)
            .checked_mul(u64::from(self.columns))
            .and_then(|start| start.checked_add(u64::from(column)))
            .ok_or(Jpeg2000Error::Overflow {
                context: "precinct index",
            })?;
        u32::try_from(index)
            .map(Some)
            .map_err(|_| Jpeg2000Error::Overflow {
                context: "precinct index",
            })
    }

    /// Tests one axis of the Annex B.12.1.3 precinct-corner condition.
    fn starts(
        self,
        position: u32,
        tile_origin: u32,
        stride: u64,
        origin: u32,
        exponent: u32,
    ) -> bool {
        let stride = stride.max(1);
        if u64::from(position).checked_rem(stride) == Some(0) {
            return true;
        }
        if position != tile_origin {
            return false;
        }
        // The level starts inside a precinct, so the tile's first column or
        // row carries that precinct's only corner inside the tile.
        let span = 1u64
            .checked_shl(exponent.min(u64::BITS.saturating_sub(1)))
            .unwrap_or(1);
        u64::from(origin).checked_rem(span) != Some(0)
    }

    /// Returns a precinct column or row index for one axis of a position.
    fn cell(
        self,
        position: u32,
        sampling: u64,
        exponent: u32,
        origin: u32,
    ) -> Result<u32, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "precinct index",
        };
        let scaled = u64::from(position).div_ceil(sampling.max(1));
        let shift = exponent.min(u64::BITS.saturating_sub(1));
        let cell = scaled
            .checked_shr(shift)
            .and_then(|cell| cell.checked_sub(u64::from(origin).checked_shr(shift)?))
            .ok_or_else(overflow)?;
        u32::try_from(cell).map_err(|_| overflow())
    }
}

/// Walks the reference-grid positions of a tile at a fixed stride.
///
/// Successive positions advance to the next multiple of the stride, so the
/// first step from an unaligned tile origin is shorter than the rest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PositionScan {
    start: u32,
    end: u32,
    stride: u64,
}

impl PositionScan {
    /// Scans `[start, end)` in steps landing on multiples of `stride`.
    pub(crate) fn new(start: u32, end: u32, stride: u64) -> Self {
        Self {
            start,
            end,
            stride: stride.max(1),
        }
    }

    /// Returns each visited position in increasing order.
    pub(crate) fn positions(self) -> impl Iterator<Item = u32> {
        let mut next = Some(self.start);
        core::iter::from_fn(move || {
            let position = next.filter(|value| *value < self.end)?;
            let advance = u64::from(position)
                .checked_rem(self.stride)
                .and_then(|offset| self.stride.checked_sub(offset))
                .unwrap_or(self.stride);
            next = u32::try_from(u64::from(position).saturating_add(advance)).ok();
            Some(position)
        })
    }
}

/// Returns `value << exponent`, or `Overflow` when it does not fit.
fn shifted(value: u64, exponent: u32) -> Result<u64, Jpeg2000Error> {
    value
        .checked_shl(exponent)
        .filter(|shifted| exponent < u64::BITS && shifted.checked_shr(exponent) == Some(value))
        .ok_or(Jpeg2000Error::Overflow {
            context: "precinct projection",
        })
}
