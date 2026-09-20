//! Validated page geometry and logical-device to bitmap mappings.

use num_traits::ToPrimitive;
use pdf_document::page::PdfPage;
use pdf_graphics::{
    point::Point,
    rect::Rect,
    transform::{Transform, TransformError},
};

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
    device_size: [f32; 2],
    /// Affine mapping from PDF page coordinates to logical device coordinates.
    page_to_device: Transform,
    /// Inverse affine mapping from logical device coordinates to PDF page coordinates.
    device_to_page: Transform,
}

impl PageViewport {
    /// Fits explicit bounds, or the page media box, independently along each axis.
    /// Missing bounds use the device size. Supplied invalid bounds are rejected.
    pub fn from_page(
        page: &PdfPage,
        bounds_override: Option<&Rect>,
        device_size: [f32; 2],
    ) -> Result<Self, ViewportError> {
        let [width, height] = device_size;
        validate_size(device_size)?;
        let (bounds, rotation) = page_bounds(page, bounds_override, || Rect::new(width, height))?;
        let sideways = rotation == 90 || rotation == 270;
        let (unrotated_width, unrotated_height) = if sideways {
            (height, width)
        } else {
            (width, height)
        };
        let sx = unrotated_width / bounds.width();
        let sy = unrotated_height / bounds.height();
        let base = Transform::from_row(
            sx,
            0.0,
            0.0,
            -sy,
            -bounds.left * sx,
            unrotated_height + bounds.top * sy,
        );
        let turn = match rotation {
            0 => Transform::identity(),
            90 => Transform::from_row(0.0, 1.0, -1.0, 0.0, width, 0.0),
            180 => Transform::from_row(-1.0, 0.0, 0.0, -1.0, width, height),
            270 => Transform::from_row(0.0, -1.0, 1.0, 0.0, 0.0, height),
            _ => return Err(ViewportError::Bounds),
        };
        let page_to_device = turn.post_concatenated(&base);
        let device_to_page = page_to_device.try_inverse()?;
        Ok(Self {
            bounds,
            device_size,
            page_to_device,
            device_to_page,
        })
    }

    /// Returns the page's displayed width and height in PDF points.
    ///
    /// Uses the CropBox, then the MediaBox, and swaps the axes for a sideways `/Rotate`
    /// so hosts size their containers with the same geometry `from_page` renders with.
    /// Pages without either box are rejected.
    pub fn page_size(page: &PdfPage) -> Result<[f32; 2], ViewportError> {
        if page.crop_box.is_none() && page.media_box.is_none() {
            return Err(ViewportError::Bounds);
        }
        let (bounds, rotation) = page_bounds(page, None, || Rect::new(1.0, 1.0))?;
        let (width, height) = (bounds.width(), bounds.height());
        if width <= 0.0 || height <= 0.0 {
            return Err(ViewportError::Bounds);
        }
        Ok(if rotation == 90 || rotation == 270 {
            [height, width]
        } else {
            [width, height]
        })
    }

    /// Returns the fitted bounds in PDF page coordinates.
    pub fn bounds(&self) -> &Rect {
        &self.bounds
    }

    /// Returns the logical device dimensions.
    pub fn device_size(&self) -> [f32; 2] {
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
        if [rect.left, rect.top, rect.right, rect.bottom]
            .iter()
            .any(|v| !v.is_finite())
        {
            return Err(ViewportError::Bounds);
        }
        let rect = rect.normalized();
        if !rect.is_valid() {
            return Err(ViewportError::Bounds);
        }
        for point in [
            Point::new(rect.left, rect.top),
            Point::new(rect.right, rect.top),
            Point::new(rect.right, rect.bottom),
            Point::new(rect.left, rect.bottom),
        ] {
            self.map_page_point(point)?;
        }
        Ok(self.page_to_device.map_rect(&rect).normalized())
    }

    /// Maps a device-space movement without applying the page origin translation.
    pub fn map_device_delta(&self, delta: Point) -> Result<Point, ViewportError> {
        let mut linear = self.device_to_page;
        linear.tx = 0.0;
        linear.ty = 0.0;
        Ok(linear.try_map_point(delta)?)
    }
}

/// Maps logical device geometry into a bitmap, independently of page or host layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasViewport {
    device_size: [f32; 2],
    backing_size: [u32; 2],
    device_to_backing: Transform,
}

impl CanvasViewport {
    /// Derives scaling from the actual backing dimensions, including pixel rounding.
    pub fn new(device_size: [f32; 2], backing_size: [u32; 2]) -> Result<Self, ViewportError> {
        validate_size(device_size)?;
        let [dw, dh] = device_size;
        let [w, h] = backing_size;
        let mapping = Transform::from_scale(
            w.to_f32().ok_or(ViewportError::Dimensions)? / dw,
            h.to_f32().ok_or(ViewportError::Dimensions)? / dh,
        );
        Self::with_transform(device_size, backing_size, mapping)
    }

    /// Uses an explicit device-to-backing mapping, such as a cropped recording origin.
    pub fn with_transform(
        device_size: [f32; 2],
        backing_size: [u32; 2],
        device_to_backing: Transform,
    ) -> Result<Self, ViewportError> {
        validate_size(device_size)?;
        if backing_size.contains(&0) {
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
    pub fn device_size(&self) -> [f32; 2] {
        self.device_size
    }

    /// Returns the bitmap dimensions.
    pub fn backing_size(&self) -> [u32; 2] {
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

/// Resolves the page box and normalized `/Rotate` shared by rendering and layout.
fn page_bounds(
    page: &PdfPage,
    bounds_override: Option<&Rect>,
    fallback: impl FnOnce() -> Rect,
) -> Result<(Rect, i32), ViewportError> {
    let bounds = bounds_override
        .or(page.crop_box.as_ref())
        .or(page.media_box.as_ref())
        .copied()
        .unwrap_or_else(fallback);
    if !bounds.is_valid() || !bounds.width().is_finite() || !bounds.height().is_finite() {
        return Err(ViewportError::Bounds);
    }
    Ok((bounds, page.rotation.unwrap_or_default().rem_euclid(360)))
}

fn validate_size(size: [f32; 2]) -> Result<(), ViewportError> {
    if size.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err(ViewportError::Dimensions);
    }
    Ok(())
}
