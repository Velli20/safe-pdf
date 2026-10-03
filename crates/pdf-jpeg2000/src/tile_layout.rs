//! Reference-grid and component-grid geometry of one tile.
//!
//! Annex B.3 places tile `t` on the reference grid from the SIZ tile origin and
//! step, clipped to the image, and projects it onto each component's sampled
//! grid. Both results are derived on demand from the borrowed SIZ table, so no
//! per-tile geometry is stored for tiles that are never decoded.

use pdf_graphics::Size;

use crate::{
    Jpeg2000Error,
    codestream::{ComponentInfo, SizeHeader},
    region::Region,
};

/// One component of a tile, on that component's sampled grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TileComponent {
    /// Zero-based SIZ component index.
    pub(crate) index: u16,
    /// Precision, signedness, and subsampling declared by SIZ.
    pub(crate) info: ComponentInfo,
    /// Tile extent on this component's sampled grid.
    pub(crate) region: Region,
}

/// Geometry of one tile of the SIZ tile grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TileLayout<'a> {
    size: SizeHeader<'a>,
    index: u32,
    region: Region,
}

impl<'a> TileLayout<'a> {
    /// Locates tile `index` on the reference grid.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the index lies outside the SIZ tile grid or the
    /// tile's reference-grid position cannot be represented.
    pub(crate) fn new(size: &SizeHeader<'a>, index: u32) -> Result<Self, Jpeg2000Error> {
        let grid = size.tile_grid();
        let columns = grid.width;
        let overflow = || Jpeg2000Error::Overflow {
            context: "tile grid position",
        };
        let (Some(column), Some(row)) = (index.checked_rem(columns), index.checked_div(columns))
        else {
            return Err(overflow());
        };
        if row >= grid.height {
            return Err(overflow());
        }
        Ok(Self {
            size: *size,
            index,
            region: Self::place(size, column, row)?,
        })
    }

    /// Returns the tile's reference-grid extent clipped to the image.
    fn place(size: &SizeHeader<'a>, column: u32, row: u32) -> Result<Region, Jpeg2000Error> {
        let origin = size.tile_origin();
        let step = size.tile_size();
        let image = size.image_region();
        let overflow = || Jpeg2000Error::Overflow {
            context: "tile grid position",
        };
        let edge = |origin: u32, step: u32, cell: u32| {
            cell.checked_mul(step)
                .and_then(|offset| origin.checked_add(offset))
                .ok_or_else(overflow)
        };
        let x0 = edge(origin.x, step.width, column)?;
        let y0 = edge(origin.y, step.height, row)?;
        let x1 = edge(origin.x, step.width, column.saturating_add(1))?;
        let y1 = edge(origin.y, step.height, row.saturating_add(1))?;
        Ok(Region::new(x0, y0, x1, y1).intersect(image))
    }

    /// Returns the tile's extent on the reference grid.
    pub(crate) fn region(self) -> Region {
        self.region
    }

    /// Returns every component of the tile in SIZ order.
    pub(crate) fn components(&self) -> impl Iterator<Item = TileComponent> + '_ {
        let layout = *self;
        self.size
            .components()
            .enumerate()
            .map(move |(index, info)| {
                let index = u16::try_from(index).unwrap_or(u16::MAX);
                layout.project(index, info)
            })
    }

    /// Projects the tile onto one component's sampled grid.
    fn project(self, index: u16, info: ComponentInfo) -> TileComponent {
        TileComponent {
            index,
            info,
            region: self.region.sampled(Size {
                width: info.x_subsampling,
                height: info.y_subsampling,
            }),
        }
    }
}
