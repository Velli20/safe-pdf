//! Borrowed tile samples delivered incrementally to a PDF image consumer.
//!
//! A tile corresponds to the image-grid partition declared by SIZ. Component
//! planes may be subsampled and signed, and sample views exist only during a
//! [`TileSink`] callback.

use crate::{Jpeg2000Error, region::Region};

/// Rectangle in the Part 1 reference grid.
///
/// Tile and component planes can have different bounds because SIZ permits
/// subsampling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileBounds {
    /// Horizontal origin in reference-grid coordinates.
    pub x: u32,
    /// Vertical origin in reference-grid coordinates.
    pub y: u32,
    /// Width in the relevant tile or component grid.
    pub width: u32,
    /// Height in the relevant tile or component grid.
    pub height: u32,
}

/// Borrowed reconstructed samples for one component plane.
///
/// `I32` supports common PDF sample precisions with smaller working storage;
/// `I64` can hold the full Part 1 range through 38-bit component precision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SampleData<'a> {
    /// Signed 32-bit reconstructed component samples.
    I32(&'a [i32]),
    /// Signed 64-bit reconstructed component samples.
    I64(&'a [i64]),
}

impl TileBounds {
    /// Returns the bounds of a decoded region on whichever grid it uses.
    pub(crate) fn from_region(region: Region) -> Self {
        let origin = region.origin();
        let extent = region.size();
        Self {
            x: origin.x,
            y: origin.y,
            width: extent.width,
            height: extent.height,
        }
    }
}

impl SampleData<'_> {
    /// Returns the number of sample elements in this borrowed plane.
    pub fn len(&self) -> usize {
        match self {
            Self::I32(samples) => samples.len(),
            Self::I64(samples) => samples.len(),
        }
    }

    /// Returns whether the borrowed sample plane contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One reconstructed tile-component plane before display colour conversion.
///
/// Bounds and row stride describe the component grid, which may differ from the
/// tile's reference grid, reduced by any discarded resolution levels. The samples are borrowed only for a sink call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentPlane<'a> {
    component_index: u16,
    bounds: TileBounds,
    row_stride: usize,
    precision: u8,
    signed: bool,
    discarded_levels: u8,
    samples: SampleData<'a>,
}

impl<'a> ComponentPlane<'a> {
    /// Builds a plane view over one reconstructed component of a tile.
    pub(crate) fn new(
        component_index: u16,
        bounds: TileBounds,
        row_stride: usize,
        precision: u8,
        signed: bool,
        discarded_levels: u8,
        samples: SampleData<'a>,
    ) -> Self {
        Self {
            component_index,
            bounds,
            row_stride,
            precision,
            signed,
            discarded_levels,
            samples,
        }
    }

    /// Returns the zero-based SIZ component index.
    pub fn component_index(&self) -> u16 {
        self.component_index
    }

    /// Returns this component's sampled tile bounds.
    pub fn bounds(&self) -> TileBounds {
        self.bounds
    }

    /// Returns the number of sample elements between adjacent rows.
    pub fn row_stride(&self) -> usize {
        self.row_stride
    }

    /// Returns the component precision in bits declared by SIZ.
    pub fn precision(&self) -> u8 {
        self.precision
    }

    /// Returns how many of the component's highest resolution levels were
    /// discarded.
    ///
    /// Each discarded level halves the component grid, so the bounds and
    /// samples cover the component at `1 / 2^levels` of its full extent.
    pub fn discarded_levels(&self) -> u8 {
        self.discarded_levels
    }

    /// Returns whether SIZ marks this component as signed.
    pub fn is_signed(&self) -> bool {
        self.signed
    }

    /// Borrows reconstructed samples in their signed integer representation.
    pub fn samples(&self) -> SampleData<'a> {
        self.samples
    }
}

/// One complete reconstructed tile made available to a [`TileSink`].
///
/// The component slice and its sample planes are temporary decoder views that a
/// PDF renderer may consume incrementally.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileView<'a> {
    index: u32,
    bounds: TileBounds,
    components: &'a [ComponentPlane<'a>],
}

impl<'a> TileView<'a> {
    /// Builds a tile view over the planes of one reconstructed tile.
    pub(crate) fn new(
        index: u32,
        bounds: TileBounds,
        components: &'a [ComponentPlane<'a>],
    ) -> Self {
        Self {
            index,
            bounds,
            components,
        }
    }

    /// Returns the zero-based SIZ tile index.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Returns tile bounds in the Part 1 reference grid.
    pub fn bounds(&self) -> TileBounds {
        self.bounds
    }

    /// Borrows all reconstructed component planes for this tile.
    pub fn components(&self) -> &'a [ComponentPlane<'a>] {
        self.components
    }
}

/// Receives complete reconstructed tiles without requiring a full image buffer.
///
/// The decoder calls this trait once per tile, after that tile's inverse
/// transforms and level shift. Implementations may map Part 1 components to
/// PDF pixels or copy them elsewhere, but must not retain the borrowed sample
/// slices beyond the call.
pub trait TileSink {
    /// Accepts a tile's borrowed component planes.
    ///
    /// # Errors
    ///
    /// Return [`Jpeg2000Error::Output`] when output storage or a downstream
    /// renderer cannot accept the tile. The decoder propagates the error.
    fn write_tile(&mut self, tile: TileView<'_>) -> Result<(), Jpeg2000Error>;
}
