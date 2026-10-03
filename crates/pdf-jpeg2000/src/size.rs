//! SIZ component descriptors and image geometry.

use crate::marker_reader::{Marker, MarkerSegment};
use crate::marker_site::MarkerSite;
use crate::{Jpeg2000Error, Resource, limits::DecoderLimits, region::Region};
use pdf_graphics::{Size, point::Point};
use pdf_utils::BitReader;

/// Bytes in one SIZ component descriptor (Ssiz, XRsiz, YRsiz).
const SIZ_COMPONENT_BYTES: usize = 3;
/// Bytes in the fixed fields of a SIZ marker body, before component descriptors.
const SIZ_FIXED_BYTES: usize = 36;
/// Greatest source component precision defined by Part 1.
pub(crate) const MAX_PRECISION: u8 = 38;
/// Ssiz bit marking a component as signed.
const SIGNED_SAMPLES: u8 = 0x80;
/// Ssiz bits holding the precision, one less than the bit depth.
const PRECISION_MASK: u8 = 0x7f;
/// Rsiz bit reserved for Part 2 codestream extensions.
const EXTENDED_PROFILE: u16 = 0x8000;

/// Image dimensions and source component count shared by SIZ and ihdr.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImageShape {
    pub(crate) size: Size<u32>,
    pub(crate) components: u16,
}

/// One axis of the SIZ image and tile grids.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GridAxis {
    image_end: u32,
    image_origin: u32,
    tile_origin: u32,
    tile_span: u32,
}

impl GridAxis {
    /// Checks that the first tile covers the image origin.
    fn is_valid(self) -> bool {
        self.tile_span > 0
            && self.image_end > self.image_origin
            && self.tile_origin <= self.image_origin
            && self.tile_span > self.image_origin.saturating_sub(self.tile_origin)
    }

    /// Returns the image span on this axis.
    fn image_span(self) -> u32 {
        self.image_end.saturating_sub(self.image_origin)
    }

    /// Counts tile-grid intervals intersecting this image axis.
    fn tile_count(self) -> u32 {
        self.image_end
            .saturating_sub(self.tile_origin)
            .div_ceil(self.tile_span.max(1))
    }
}

/// One component descriptor from a SIZ marker's component table.
///
/// Precision includes a sign bit for signed samples.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentInfo {
    /// Component sample precision in bits, as declared by SIZ.
    pub precision: u8,
    /// Whether source samples are signed.
    pub signed: bool,
    /// Horizontal subsampling factor relative to the reference grid.
    pub x_subsampling: u8,
    /// Vertical subsampling factor relative to the reference grid.
    pub y_subsampling: u8,
}

impl ComponentInfo {
    /// Checks the Part 1 precision range and non-zero subsampling factors.
    fn is_valid(self) -> bool {
        self.precision <= MAX_PRECISION && self.x_subsampling > 0 && self.y_subsampling > 0
    }
}

impl From<&[u8; SIZ_COMPONENT_BYTES]> for ComponentInfo {
    fn from([depth, x_subsampling, y_subsampling]: &[u8; SIZ_COMPONENT_BYTES]) -> Self {
        Self {
            precision: (depth & PRECISION_MASK).saturating_add(1),
            signed: depth & SIGNED_SAMPLES != 0,
            x_subsampling: *x_subsampling,
            y_subsampling: *y_subsampling,
        }
    }
}

/// Borrowed SIZ image and tile geometry from the main header.
///
/// The per-component descriptor table is retained as source bytes, so a
/// component descriptor can be interpreted without allocating a vector for
/// every PDF image. All coordinates use the Part 1 reference grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SizeHeader<'a> {
    site: MarkerSite,
    profile: u16,
    axes: [GridAxis; 2],
    component_count: u16,
    component_table: &'a [u8],
}

impl<'a> SizeHeader<'a> {
    /// Returns the SIZ `Rsiz` profile value.
    pub fn profile(&self) -> u16 {
        self.profile
    }

    /// Returns the image extent after applying the reference-grid origin.
    pub fn image_size(&self) -> Size<u32> {
        let [horizontal, vertical] = self.axes;
        Size {
            width: horizontal.image_span(),
            height: vertical.image_span(),
        }
    }

    /// Returns the reference-grid coordinate of the image's first sample.
    pub fn image_origin(&self) -> Point<u32> {
        let [horizontal, vertical] = self.axes;
        Point {
            x: horizontal.image_origin,
            y: vertical.image_origin,
        }
    }

    /// Returns the SIZ tile-grid step on both axes.
    pub fn tile_size(&self) -> Size<u32> {
        let [horizontal, vertical] = self.axes;
        Size {
            width: horizontal.tile_span,
            height: vertical.tile_span,
        }
    }

    /// Returns the image on the reference grid as `[XOsiz, Xsiz) x [YOsiz, Ysiz)`.
    pub(crate) fn image_region(&self) -> Region {
        let [horizontal, vertical] = self.axes;
        Region::new(
            horizontal.image_origin,
            vertical.image_origin,
            horizontal.image_end,
            vertical.image_end,
        )
    }

    /// Returns the reference-grid origin of the SIZ tile grid.
    pub fn tile_origin(&self) -> Point<u32> {
        let [horizontal, vertical] = self.axes;
        Point {
            x: horizontal.tile_origin,
            y: vertical.tile_origin,
        }
    }

    /// Returns the number of tiles across and down the image.
    pub fn tile_grid(&self) -> Size<u32> {
        let [horizontal, vertical] = self.axes;
        Size {
            width: horizontal.tile_count(),
            height: vertical.tile_count(),
        }
    }

    /// Returns the number of component descriptors declared by SIZ.
    pub fn component_count(&self) -> u16 {
        self.component_count
    }

    /// Returns every component descriptor in SIZ order.
    pub fn components(&self) -> impl Iterator<Item = ComponentInfo> + 'a {
        let (descriptors, _) = self.component_table.as_chunks::<SIZ_COMPONENT_BYTES>();
        descriptors.iter().map(ComponentInfo::from)
    }

    /// Returns a component descriptor by its zero-based SIZ index.
    ///
    /// # Errors
    ///
    /// Returns `InvalidMarker` if the index lies outside the SIZ table.
    pub fn component(&self, index: u16) -> Result<ComponentInfo, Jpeg2000Error> {
        self.components()
            .nth(index.into())
            .ok_or_else(|| self.site.invalid("component index exceeds SIZ count"))
    }

    /// Returns the image shape for comparison with a container's image header.
    pub(crate) fn shape(&self) -> ImageShape {
        ImageShape {
            size: self.image_size(),
            components: self.component_count,
        }
    }

    /// Returns the reference-grid pixel count in a width that cannot overflow.
    fn pixel_count(&self) -> u64 {
        let extent = self.image_size();
        u64::from(extent.width).saturating_mul(u64::from(extent.height))
    }

    /// Returns the total number of tiles implied by the SIZ tile grid.
    pub(crate) fn tile_count(&self) -> u64 {
        let grid = self.tile_grid();
        u64::from(grid.width).saturating_mul(u64::from(grid.height))
    }

    /// Checks resource bounds before any tile workspace is allocated.
    pub(crate) fn limited_tiles(&self, limits: DecoderLimits) -> Result<u32, Jpeg2000Error> {
        Resource::Components.check(self.component_count.into(), limits.max_components.into())?;
        Resource::Pixels.check(self.pixel_count(), limits.max_pixels)?;
        let tiles = self.tile_count();
        Resource::Tiles.check(tiles, limits.max_tiles.into())?;
        tiles.try_into().map_err(|_| Jpeg2000Error::Overflow {
            context: "SIZ tile count",
        })
    }

    /// Checks the image extent and the tile containing its top-left sample.
    fn validate_grid(&self) -> Result<(), Jpeg2000Error> {
        if self.axes.into_iter().all(GridAxis::is_valid) {
            return Ok(());
        }
        self.site.reject_invalid("invalid image or tile geometry")
    }

    /// Checks all component descriptors without allocating component metadata.
    fn validate_components(&self) -> Result<(), Jpeg2000Error> {
        let (descriptors, remainder) = self.component_table.as_chunks::<SIZ_COMPONENT_BYTES>();
        if self.component_count > 0
            && remainder.is_empty()
            && descriptors.len() == usize::from(self.component_count)
            && self.components().all(ComponentInfo::is_valid)
        {
            return Ok(());
        }
        self.site.reject_invalid("invalid component descriptor")
    }
}

impl<'a> TryFrom<MarkerSegment<'a>> for SizeHeader<'a> {
    type Error = Jpeg2000Error;

    fn try_from(marker: MarkerSegment<'a>) -> Result<Self, Self::Error> {
        if marker.marker() != Marker::Siz {
            return marker.site().out_of_order();
        }
        let site = marker.site();
        let offset_site = site.offset_site();
        if marker.payload().len() < SIZ_FIXED_BYTES {
            return site.reject_invalid("SIZ fixed fields are incomplete");
        }
        let mut fields = BitReader::new(marker.payload());
        let profile = fields.try_read_u16_be::<u16>()?;
        if profile & EXTENDED_PROFILE != 0 {
            return Err(Jpeg2000Error::UnsupportedProfile { profile });
        }
        let image_end = read_point(&mut fields)?;
        let image_origin = read_point(&mut fields)?;
        let tile_span = read_point(&mut fields)?;
        let tile_origin = read_point(&mut fields)?;
        let component_count = fields.try_read_u16_be::<u16>()?;
        let table_len = usize::from(component_count)
            .checked_mul(SIZ_COMPONENT_BYTES)
            .and_then(|table| SIZ_FIXED_BYTES.checked_add(table))
            .ok_or(Jpeg2000Error::Overflow {
                context: "SIZ component table",
            })?;
        if marker.payload().len() != table_len {
            return site.reject_invalid("SIZ component table length does not match Csiz");
        }
        let component_table = fields
            .remaining_from_byte()
            .ok_or_else(|| offset_site.truncated("SIZ components"))?;
        let axes = [
            GridAxis {
                image_end: image_end.x,
                image_origin: image_origin.x,
                tile_origin: tile_origin.x,
                tile_span: tile_span.x,
            },
            GridAxis {
                image_end: image_end.y,
                image_origin: image_origin.y,
                tile_origin: tile_origin.y,
                tile_span: tile_span.y,
            },
        ];
        let size = Self {
            site,
            profile,
            axes,
            component_count,
            component_table,
        };
        size.validate_grid()?;
        size.validate_components()?;
        Ok(size)
    }
}

/// Reads one horizontal and vertical SIZ coordinate pair.
fn read_point(fields: &mut BitReader<'_>) -> Result<Point<u32>, Jpeg2000Error> {
    Ok(Point {
        x: fields.try_read_u32_be::<u32>()?,
        y: fields.try_read_u32_be::<u32>()?,
    })
}
