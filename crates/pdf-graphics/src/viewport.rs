//! Validated page geometry and logical-device to bitmap mappings.

use crate::{
    point::Point,
    quad::Quad,
    rect::Rect,
    size::Size,
    transform::{Transform, TransformError},
};
use num_traits::ToPrimitive;

/// Invalid viewport geometry or incompatible rendering dimensions.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum ViewportError {
    /// Dimensions must be finite and positive.
    #[error("invalid viewport dimensions")]
    Dimensions,
    /// Supplied page bounds must have finite edges and positive dimensions.
    #[error("invalid page bounds")]
    Bounds,
    /// The backend and viewport use different logical device dimensions.
    #[error("viewport dimensions differ from the rendering target")]
    DeviceSizeMismatch,
    /// A coordinate mapping is nonfinite or cannot be inverted.
    #[error(transparent)]
    Transform(#[from] TransformError),
}

/// Maps PDF page coordinates into the logical device space consumed by backends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageViewport {
    /// PDF-space bounds fitted into the logical device viewport.
    bounds: Rect,
    /// Width and height of the target's logical device coordinate space.
    device_size: Size,
    /// Affine mapping from PDF page coordinates to logical device coordinates.
    page_to_device: Transform,
    /// Inverse affine mapping from logical device coordinates to PDF page coordinates.
    device_to_page: Transform,
}

impl PageViewport {
    /// Fits page `bounds` into `device_size` independently along each axis, applying a
    /// clockwise `rotation` in degrees.
    ///
    /// Rejects invalid device dimensions, invalid bounds, rotations that are not a
    /// multiple of 90 degrees, and mappings that cannot be inverted.
    pub fn new(bounds: Rect, rotation: i32, device_size: Size) -> Result<Self, ViewportError> {
        if !device_size.validate() {
            return Err(ViewportError::Dimensions);
        }
        if !bounds.is_valid() {
            return Err(ViewportError::Bounds);
        }
        let unrotated = device_size.quarter_turned(rotation);
        let sx = unrotated.width / bounds.width();
        let sy = unrotated.height / bounds.height();
        let base = Transform::from_row(
            sx,
            0.0,
            0.0,
            -sy,
            -bounds.left * sx,
            unrotated.height + bounds.top * sy,
        );
        let turn =
            Transform::from_quarter_turn(rotation, device_size).ok_or(ViewportError::Bounds)?;
        let page_to_device = turn.post_concatenated(&base);
        let device_to_page = page_to_device.try_inverse()?;
        Ok(Self {
            bounds,
            device_size,
            page_to_device,
            device_to_page,
        })
    }

    /// Returns the fitted bounds in PDF page coordinates.
    pub fn bounds(&self) -> &Rect {
        &self.bounds
    }

    /// Returns the logical device dimensions.
    pub fn device_size(&self) -> Size {
        self.device_size
    }

    /// Returns the initial page transform; backends must not reapply it to paths.
    pub fn page_to_device(&self) -> &Transform {
        &self.page_to_device
    }

    /// Returns the inverse page mapping.
    pub fn device_to_page(&self) -> &Transform {
        &self.device_to_page
    }

    /// Maps a page point into logical device coordinates.
    pub fn map_page_point(&self, point: Point) -> Result<Point, ViewportError> {
        Ok(self.page_to_device.try_map_point(point)?)
    }

    /// Maps a logical device point into PDF page coordinates.
    pub fn map_device_point(&self, point: Point) -> Result<Point, ViewportError> {
        Ok(self.device_to_page.try_map_point(point)?)
    }

    /// Maps all corners of a finite page rectangle into normalized device bounds.
    pub fn map_rect(&self, rect: &Rect) -> Result<Rect, ViewportError> {
        let mapped = self.page_to_device.try_map_rect(rect)?;
        // The edges are finite once mapping succeeds, so normalizing keeps them.
        if !rect.normalized().is_valid() {
            return Err(ViewportError::Bounds);
        }
        Ok(mapped)
    }

    /// Maps the corners of an `f64` page quad and returns their enclosing device bounds.
    /// Rejects corners without a finite `f32` representation or a nonfinite result.
    pub fn map_quad(&self, quad: &Quad<f64>) -> Result<Rect<f64>, ViewportError> {
        let mut corners = quad.corners;
        for corner in &mut corners {
            let page = corner.to_f32().ok_or(TransformError::NonFinite)?;
            *corner = self.map_page_point(page)?.into();
        }
        Ok(Quad { corners }.bounds())
    }

    /// Maps a y-down local frame anchored at page point `origin` into logical device
    /// coordinates: local `(x, y)` is page `(origin.x + x, origin.y - y)`. Rejects a
    /// nonfinite result.
    pub fn local_frame(&self, origin: Point) -> Result<Transform, ViewportError> {
        let local_to_page = Transform::from_row(1.0, 0.0, 0.0, -1.0, origin.x, origin.y);
        let local_to_device = self.page_to_device.post_concatenated(&local_to_page);
        local_to_device.validate()?;
        Ok(local_to_device)
    }

    /// Maps a device-space movement without applying the page origin translation.
    pub fn map_device_delta(&self, delta: Point) -> Result<Point, ViewportError> {
        Ok(self.device_to_page.linear().try_map_point(delta)?)
    }
}

/// Maps logical device geometry into a bitmap, independently of page or host layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasViewport {
    device_size: Size,
    backing_size: Size<u32>,
    device_to_backing: Transform,
}

impl CanvasViewport {
    /// Derives scaling from the actual backing dimensions, including pixel rounding.
    pub fn new(device_size: Size, backing_size: Size<u32>) -> Result<Self, ViewportError> {
        if !device_size.validate() {
            return Err(ViewportError::Dimensions);
        }
        let mapping = Transform::from_scale(
            backing_size
                .width
                .to_f32()
                .ok_or(ViewportError::Dimensions)?
                / device_size.width,
            backing_size
                .height
                .to_f32()
                .ok_or(ViewportError::Dimensions)?
                / device_size.height,
        );
        Self::with_transform(device_size, backing_size, mapping)
    }

    /// Uses an explicit device-to-backing mapping, such as a cropped recording origin.
    pub fn with_transform(
        device_size: Size,
        backing_size: Size<u32>,
        device_to_backing: Transform,
    ) -> Result<Self, ViewportError> {
        if !device_size.validate() || backing_size.width == 0 || backing_size.height == 0 {
            return Err(ViewportError::Dimensions);
        }
        device_to_backing.try_inverse()?;
        Ok(Self {
            device_size,
            backing_size,
            device_to_backing,
        })
    }

    /// Returns the dimensions exposed by the canvas backend.
    pub fn device_size(&self) -> Size {
        self.device_size
    }

    /// Returns the bitmap dimensions.
    pub fn backing_size(&self) -> Size<u32> {
        self.backing_size
    }

    /// Returns the mapping applied by the backend to logical device geometry.
    pub fn device_to_backing(&self) -> &Transform {
        &self.device_to_backing
    }

    /// Returns the logical device width of one backing pixel along the denser axis.
    pub fn hairline_width(&self) -> f32 {
        1.0 / self.device_to_backing.max_scale()
    }
}

/// Rounds a positive finite raster extent up to a representable pixel dimension.
pub fn pixel_extent(value: f32) -> Option<u32> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    value.ceil().to_u32().filter(|extent| *extent > 0)
}
