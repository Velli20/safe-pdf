//! Code-block partition of one subband precinct.
//!
//! Annex B.7 partitions a subband with a grid anchored at the subband origin,
//! using the nominal code-block exponents from SPcod or SPcoc reduced so a
//! code-block never crosses a precinct boundary. The code-blocks of one
//! precinct are the cells of that partition clipped to the precinct.

use pdf_graphics::Size;

use crate::{Jpeg2000Error, coding::CodingParameters, region::Region};

/// Amount SPcod and SPcoc subtract from each code-block exponent.
const EXPONENT_OFFSET: u8 = 2;

/// One code-block of a subband precinct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CodeBlock {
    /// Raster index of the block within its precinct.
    pub(crate) index: u32,
    /// Coefficient extent of the block within its subband.
    pub(crate) region: Region,
}

/// The code-block partition of one subband precinct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CodeBlockGrid {
    grid: crate::region::BlockGrid,
    columns: u32,
    rows: u32,
}

impl CodeBlockGrid {
    /// Partitions one subband precinct into code-blocks.
    ///
    /// `band_exponents` are the precinct exponents already expressed on the
    /// subband grid, which bound the code-block size from above.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the partition cannot be counted.
    pub(crate) fn new(
        region: Region,
        parameters: &CodingParameters<'_>,
        band_exponents: Size<u8>,
    ) -> Result<Self, Jpeg2000Error> {
        let nominal = parameters.code_block;
        let exponents = Size {
            width: effective(nominal.width, band_exponents.width),
            height: effective(nominal.height, band_exponents.height),
        };
        let grid = region.partition(exponents)?;
        Ok(Self {
            grid,
            columns: grid.columns(),
            rows: grid.rows(),
        })
    }

    /// Returns the number of code-block columns in the precinct.
    pub(crate) fn columns(self) -> u32 {
        self.columns
    }

    /// Returns the number of code-block rows in the precinct.
    pub(crate) fn rows(self) -> u32 {
        self.rows
    }

    /// Returns the number of code-blocks in the precinct.
    pub(crate) fn count(self) -> u64 {
        self.grid.count()
    }

    /// Returns every code-block of the precinct in raster order.
    pub(crate) fn blocks(self) -> impl Iterator<Item = CodeBlock> {
        self.grid
            .blocks()
            .enumerate()
            .map(|(index, block)| CodeBlock {
                index: u32::try_from(index).unwrap_or(u32::MAX),
                region: block.region,
            })
    }
}

/// Returns the code-block exponent that fits inside a precinct.
fn effective(nominal: u8, precinct: u8) -> u8 {
    nominal.saturating_add(EXPONENT_OFFSET).min(precinct)
}
