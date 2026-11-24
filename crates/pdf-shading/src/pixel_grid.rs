//! The pixel grid that function-based and mesh shadings are sampled on.

use num_traits::ToPrimitive;
use pdf_graphics::{
    Image, PixelFormat, point::Point, rect::Rect, size::Size, transform::Transform,
};

/// Offset from a pixel's corner to its center, where each sample is taken.
pub(crate) const PIXEL_CENTER: f32 = 0.5;

/// Equal pixels covering an area, at least one along each axis.
pub(crate) struct PixelGrid {
    area: Rect,
    size: Size<u16>,
}

impl PixelGrid {
    /// Covers `area` with `extent` pixels, rounded up to whole pixels and limited to
    /// `1..=max_dimension` along each axis.
    pub(crate) fn new(area: Rect, extent: Size, max_dimension: u16) -> Self {
        let pixels = |extent: f32| {
            extent
                .ceil()
                .max(1.0)
                .to_u16()
                .map_or(max_dimension, |pixels| pixels.min(max_dimension))
        };
        Self {
            area,
            size: Size::new(pixels(extent.width), pixels(extent.height)),
        }
    }

    /// Returns the number of pixels along each axis.
    pub(crate) fn size(&self) -> Size<u16> {
        self.size
    }

    /// Returns the transform mapping pixel coordinates onto the area.
    pub(crate) fn placement(&self) -> Transform {
        Transform::from_row(
            self.area.width() / f32::from(self.size.width),
            0.0,
            0.0,
            self.area.height() / f32::from(self.size.height),
            self.area.left,
            self.area.top,
        )
    }

    /// Iterates the area coordinates of the pixel centers in row-major order, the first
    /// row nearest the top of the area.
    pub(crate) fn centers(&self) -> impl Iterator<Item = Point> {
        let Size { width, height } = self.size;
        let placement = self.placement();
        (0..height).flat_map(move |row| {
            (0..width).map(move |column| {
                placement.map_point(Point::new(
                    f32::from(column) + PIXEL_CENTER,
                    f32::from(row) + PIXEL_CENTER,
                ))
            })
        })
    }

    /// Packs one row-major RGBA8 sample per pixel into an image.
    pub(crate) fn image(&self, pixels: Vec<[u8; 4]>, interpolate: bool) -> Image {
        Image {
            data: pixels.into_flattened().into(),
            width: usize::from(self.size.width),
            height: usize::from(self.size.height),
            pixel_format: PixelFormat::RGBA8888,
            interpolate,
        }
    }
}
