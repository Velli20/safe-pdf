//! Half-open rectangles on the reference, component, and subband grids.
//!
//! Annex B expresses every relation between the image, its tiles, components,
//! resolution levels, precincts, and code-blocks as a rectangle whose upper
//! bounds are exclusive. [`Region`] models that rectangle once, with checked
//! arithmetic, so each Annex B derivation stays one short named operation.
//!
//! `pdf_graphics::Rect` is not reused here: its helpers are defined only for
//! `f32` edges, and it carries neither half-open nor overflow-checked
//! semantics.

use pdf_graphics::{Size, point::Point};

use crate::Jpeg2000Error;

/// Largest dyadic exponent a Part 1 codestream can ask a region to shift by.
///
/// Decomposition levels are capped at 32 by COD, and a precinct or code-block
/// exponent is bounded by the same field width.
const MAX_EXPONENT: u32 = 32;

/// A half-open rectangle `[x0, x1) x [y0, y1)` in one JPEG 2000 grid.
///
/// Which grid a region belongs to is carried by its owner, not by the region:
/// the reference grid for tiles, a sampled grid for tile-components, and a
/// subband grid for coefficients.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Region {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl Region {
    /// Creates a region, clamping an inverted edge to an empty extent.
    ///
    /// Annex B derivations cannot invert a rectangle, and an intersection that
    /// misses simply has no samples, so clamping keeps the type total.
    pub(crate) fn new(x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        Self {
            x0,
            y0,
            x1: x1.max(x0),
            y1: y1.max(y0),
        }
    }

    /// Returns the inclusive lower corner of the region.
    pub(crate) fn origin(self) -> Point<u32> {
        Point {
            x: self.x0,
            y: self.y0,
        }
    }

    /// Returns the exclusive upper corner of the region.
    pub(crate) fn end(self) -> Point<u32> {
        Point {
            x: self.x1,
            y: self.y1,
        }
    }

    /// Returns the region's extent, which is zero on an empty axis.
    pub(crate) fn size(self) -> Size<u32> {
        Size {
            width: self.x1.saturating_sub(self.x0),
            height: self.y1.saturating_sub(self.y0),
        }
    }

    /// Returns whether the region contains no samples.
    pub(crate) fn is_empty(self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    /// Returns the sample count in a width that cannot overflow.
    pub(crate) fn area(self) -> u64 {
        let extent = self.size();
        u64::from(extent.width).saturating_mul(u64::from(extent.height))
    }

    /// Returns the sample count as a buffer length for this target.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` when the region cannot be addressed by `usize`.
    pub(crate) fn sample_count(self) -> Result<usize, Jpeg2000Error> {
        usize::try_from(self.area()).map_err(|_| Jpeg2000Error::Overflow {
            context: "region sample count",
        })
    }

    /// Returns the overlap of two regions, which may be empty.
    pub(crate) fn intersect(self, other: Self) -> Self {
        Self::new(
            self.x0.max(other.x0),
            self.y0.max(other.y0),
            self.x1.min(other.x1),
            self.y1.min(other.y1),
        )
    }

    /// Projects a reference-grid region onto a component's sampled grid.
    ///
    /// This is the Annex B tile-component rule `tcx0 = ceil(tx0 / XRsiz)`.
    pub(crate) fn sampled(self, subsampling: Size<u8>) -> Self {
        let horizontal = u32::from(subsampling.width).max(1);
        let vertical = u32::from(subsampling.height).max(1);
        Self::new(
            self.x0.div_ceil(horizontal),
            self.y0.div_ceil(vertical),
            self.x1.div_ceil(horizontal),
            self.y1.div_ceil(vertical),
        )
    }

    /// Divides a region by `2^exponent`, rounding every edge up.
    ///
    /// Resolution level `r` of a tile-component uses this with the number of
    /// decomposition levels that remain above it.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the exponent is outside the Part 1 range.
    pub(crate) fn downscale(self, exponent: u32) -> Result<Self, Jpeg2000Error> {
        let divisor = dyadic(exponent)?;
        Ok(Self::new(
            ceil_shift(self.x0, divisor),
            ceil_shift(self.y0, divisor),
            ceil_shift(self.x1, divisor),
            ceil_shift(self.y1, divisor),
        ))
    }

    /// Maps a tile-component region onto one subband of a decomposition level.
    ///
    /// This is the Annex B subband rule
    /// `tbx0 = ceil((tcx0 - 2^(n-1) * xob) / 2^n)`, where `n` counts the
    /// decompositions applied above the subband and `offset` is the subband's
    /// `(xob, yob)` pair.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the exponent is outside the Part 1 range.
    pub(crate) fn subband(self, exponent: u32, offset: Point<u32>) -> Result<Self, Jpeg2000Error> {
        let divisor = dyadic(exponent)?;
        let half = dyadic(exponent.saturating_sub(1))?;
        let half = i64::try_from(half).map_err(|_| Jpeg2000Error::Overflow {
            context: "subband divisor",
        })?;
        let bias = |factor: u32| half.saturating_mul(i64::from(factor));
        let horizontal = bias(offset.x);
        let vertical = bias(offset.y);
        Ok(Self::new(
            band_edge(self.x0, horizontal, divisor)?,
            band_edge(self.y0, vertical, divisor)?,
            band_edge(self.x1, horizontal, divisor)?,
            band_edge(self.y1, vertical, divisor)?,
        ))
    }

    /// Partitions the region with a grid anchored at the origin of its plane.
    ///
    /// Precincts and code-blocks are both defined by such a partition, so both
    /// are described by the returned [`BlockGrid`].
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if an exponent is outside the Part 1 range or the
    /// grid cannot be counted.
    pub(crate) fn partition(self, exponents: Size<u8>) -> Result<BlockGrid, Jpeg2000Error> {
        BlockGrid::new(self, exponents)
    }
}

/// One cell of a [`BlockGrid`], clipped to the partitioned region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Block {
    /// Column of the cell within the grid, counted from its first column.
    pub(crate) column: u32,
    /// Row of the cell within the grid, counted from its first row.
    pub(crate) row: u32,
    /// Samples of the partitioned region that fall inside the cell.
    pub(crate) region: Region,
}

/// An origin-anchored partition of a region into equally sized cells.
///
/// The grid is anchored at the origin of the plane rather than at the region,
/// which is what makes precinct and code-block boundaries agree across tiles
/// and resolution levels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BlockGrid {
    bounds: Region,
    exponents: Size<u8>,
    first: Point<u32>,
    columns: u32,
    rows: u32,
}

impl BlockGrid {
    /// Counts the cells of the anchored partition covering `bounds`.
    fn new(bounds: Region, exponents: Size<u8>) -> Result<Self, Jpeg2000Error> {
        let horizontal = checked_exponent(exponents.width)?;
        let vertical = checked_exponent(exponents.height)?;
        if bounds.is_empty() {
            return Ok(Self {
                bounds,
                exponents,
                first: Point { x: 0, y: 0 },
                columns: 0,
                rows: 0,
            });
        }
        let first = Point {
            x: bounds.x0 >> horizontal,
            y: bounds.y0 >> vertical,
        };
        let last = Point {
            x: bounds.x1.saturating_sub(1) >> horizontal,
            y: bounds.y1.saturating_sub(1) >> vertical,
        };
        Ok(Self {
            bounds,
            exponents,
            first,
            columns: span(first.x, last.x),
            rows: span(first.y, last.y),
        })
    }

    /// Returns the number of cell columns covering the region.
    pub(crate) fn columns(self) -> u32 {
        self.columns
    }

    /// Returns the number of cell rows covering the region.
    pub(crate) fn rows(self) -> u32 {
        self.rows
    }

    /// Returns the total number of cells covering the region.
    pub(crate) fn count(self) -> u64 {
        u64::from(self.columns).saturating_mul(u64::from(self.rows))
    }

    /// Returns the cell at a column and row of this grid, clipped to bounds.
    ///
    /// Returns `None` when the coordinates lie outside the covered cells.
    pub(crate) fn block(self, column: u32, row: u32) -> Option<Block> {
        if column >= self.columns || row >= self.rows {
            return None;
        }
        let horizontal = u32::from(self.exponents.width);
        let vertical = u32::from(self.exponents.height);
        let start =
            |first: u32, index: u32, exponent: u32| first.checked_add(index)?.checked_shl(exponent);
        let x0 = start(self.first.x, column, horizontal)?;
        let y0 = start(self.first.y, row, vertical)?;
        let cell = Region::new(
            x0,
            y0,
            saturating_next(x0, horizontal),
            saturating_next(y0, vertical),
        );
        Some(Block {
            column,
            row,
            region: self.bounds.intersect(cell),
        })
    }

    /// Returns every cell of the grid in raster order.
    pub(crate) fn blocks(self) -> impl Iterator<Item = Block> {
        (0..self.rows).flat_map(move |row| {
            (0..self.columns).filter_map(move |column| self.block(column, row))
        })
    }
}

/// Returns `2^exponent` when the exponent is inside the Part 1 range.
fn dyadic(exponent: u32) -> Result<u64, Jpeg2000Error> {
    if exponent > MAX_EXPONENT {
        return Err(Jpeg2000Error::Overflow {
            context: "dyadic grid exponent",
        });
    }
    1u64.checked_shl(exponent).ok_or(Jpeg2000Error::Overflow {
        context: "dyadic grid exponent",
    })
}

/// Checks a partition exponent taken from a marker segment.
fn checked_exponent(exponent: u8) -> Result<u32, Jpeg2000Error> {
    let exponent = u32::from(exponent);
    if exponent >= MAX_EXPONENT {
        return Err(Jpeg2000Error::Overflow {
            context: "partition exponent",
        });
    }
    Ok(exponent)
}

/// Divides a non-negative edge by a power of two, rounding up.
fn ceil_shift(edge: u32, divisor: u64) -> u32 {
    let quotient = u64::from(edge).div_ceil(divisor.max(1));
    u32::try_from(quotient).unwrap_or(u32::MAX)
}

/// Evaluates one `ceil((edge - bias) / divisor)` subband coordinate.
///
/// The bias is at most half the divisor, so the quotient of a non-negative
/// edge is never negative even though the numerator can be.
fn band_edge(edge: u32, bias: i64, divisor: u64) -> Result<u32, Jpeg2000Error> {
    let divisor = i64::try_from(divisor).map_err(|_| Jpeg2000Error::Overflow {
        context: "subband divisor",
    })?;
    let numerator = i64::from(edge).saturating_sub(bias);
    let floor = numerator.div_euclid(divisor.max(1));
    let quotient = floor.saturating_add(i64::from(numerator.rem_euclid(divisor.max(1)) != 0));
    u32::try_from(quotient.max(0)).map_err(|_| Jpeg2000Error::Overflow {
        context: "subband coordinate",
    })
}

/// Counts the inclusive cells between two grid indices.
fn span(first: u32, last: u32) -> u32 {
    last.saturating_sub(first).saturating_add(1)
}

/// Returns the exclusive end of a cell that starts at `origin`.
fn saturating_next(origin: u32, exponent: u32) -> u32 {
    1u32.checked_shl(exponent)
        .and_then(|extent| origin.checked_add(extent))
        .unwrap_or(u32::MAX)
}
