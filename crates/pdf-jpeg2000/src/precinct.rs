//! Precinct partition of a resolution level and of its subbands.
//!
//! Annex B.6 partitions a resolution level into precincts anchored at the
//! origin of the resolution grid, with one `PPx`/`PPy` exponent pair per level
//! taken from SPcod or SPcoc. A precinct groups the code-blocks that share one
//! packet, so the same partition, reduced by one on each axis above level
//! zero, also selects the matching region of every subband.

use pdf_graphics::Size;

use crate::{
    Jpeg2000Error,
    coding::CodingParameters,
    region::{BlockGrid, Region},
    resolution::{Resolution, Subband},
};

/// Precinct exponent used when SPcod or SPcoc omits the precinct table.
const DEFAULT_EXPONENT: u8 = 15;
/// Mask selecting `PPx` from one precinct-size byte.
const HORIZONTAL_MASK: u8 = 0x0f;
/// Shift selecting `PPy` from one precinct-size byte.
const VERTICAL_SHIFT: u32 = 4;

/// The per-resolution precinct exponents of one tile-component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PrecinctSizes<'a> {
    table: Option<&'a [u8]>,
}

impl<'a> PrecinctSizes<'a> {
    /// Reads the precinct table of a COD or COC segment.
    pub(crate) fn new(parameters: &CodingParameters<'a>) -> Self {
        Self {
            table: parameters.precincts,
        }
    }

    /// Returns the `PPx`/`PPy` exponents for one resolution level.
    ///
    /// # Errors
    ///
    /// Returns `InvalidMarker` if the table omits the level, or if a level
    /// above zero declares a zero exponent, which Annex A forbids.
    pub(crate) fn exponents(&self, resolution: u8) -> Result<Size<u8>, Jpeg2000Error> {
        let Some(table) = self.table else {
            return Ok(Size {
                width: DEFAULT_EXPONENT,
                height: DEFAULT_EXPONENT,
            });
        };
        let byte = table
            .get(usize::from(resolution))
            .ok_or(Jpeg2000Error::UnsupportedFeature {
                feature: "precinct table shorter than the decomposition count",
            })?;
        let exponents = Size {
            width: byte & HORIZONTAL_MASK,
            height: byte >> VERTICAL_SHIFT,
        };
        if resolution > 0 && (exponents.width == 0 || exponents.height == 0) {
            return Err(Jpeg2000Error::UnsupportedFeature {
                feature: "zero precinct exponent above resolution level zero",
            });
        }
        Ok(exponents)
    }
}

/// One precinct of a resolution level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Precinct {
    /// Raster index of the precinct within its resolution level.
    pub(crate) index: u32,
    /// Column of the precinct within the resolution's precinct grid.
    pub(crate) column: u32,
    /// Row of the precinct within the resolution's precinct grid.
    pub(crate) row: u32,
    /// Extent of the precinct on the resolution grid.
    pub(crate) region: Region,
}

/// The precinct partition of one resolution level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PrecinctGrid {
    grid: BlockGrid,
    exponents: Size<u8>,
    band_exponents: Size<u8>,
}

impl PrecinctGrid {
    /// Partitions a resolution level into precincts.
    ///
    /// # Errors
    ///
    /// Returns a structural error if the precinct table is invalid, and
    /// `Overflow` if the partition cannot be counted.
    pub(crate) fn new(
        resolution: Resolution,
        sizes: &PrecinctSizes<'_>,
    ) -> Result<Self, Jpeg2000Error> {
        let exponents = sizes.exponents(resolution.index())?;
        // Above level zero a subband grid is half the resolution grid, so the
        // partition that selects one precinct's coefficients loses one
        // exponent on each axis.
        let reduction = u8::from(resolution.index() > 0);
        Ok(Self {
            grid: resolution.region().partition(exponents)?,
            exponents,
            band_exponents: Size {
                width: exponents.width.saturating_sub(reduction),
                height: exponents.height.saturating_sub(reduction),
            },
        })
    }

    /// Returns the precinct exponents on the resolution grid.
    pub(crate) fn exponents(self) -> Size<u8> {
        self.exponents
    }

    /// Returns the precinct exponents on a subband grid of this level.
    pub(crate) fn band_exponents(self) -> Size<u8> {
        self.band_exponents
    }

    /// Returns the number of precincts in the level.
    pub(crate) fn count(self) -> u64 {
        self.grid.count()
    }

    /// Returns the number of precinct columns in the level.
    pub(crate) fn columns(self) -> u32 {
        self.grid.columns()
    }

    /// Returns the number of precinct rows in the level.
    pub(crate) fn rows(self) -> u32 {
        self.grid.rows()
    }

    /// Returns one precinct of the level by its raster index.
    pub(crate) fn precinct(self, index: u32) -> Option<Precinct> {
        let columns = self.grid.columns();
        let column = index.checked_rem(columns)?;
        let row = index.checked_div(columns)?;
        let block = self.grid.block(column, row)?;
        Some(Precinct {
            index,
            column: block.column,
            row: block.row,
            region: block.region,
        })
    }

    /// Returns the coefficients of one subband that belong to a precinct.
    ///
    /// The subband partition uses the same grid coordinates as the resolution
    /// partition, so a precinct selects the matching cell of every band.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the band partition cannot be counted.
    pub(crate) fn band_region(
        self,
        band: &Subband,
        precinct: &Precinct,
    ) -> Result<Region, Jpeg2000Error> {
        let grid = band.region.partition(self.band_exponents)?;
        Ok(grid
            .block(precinct.column, precinct.row)
            .map_or_else(Region::default, |block| block.region))
    }
}
