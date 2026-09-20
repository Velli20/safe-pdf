//! Shared mappings between existing device geometry, browser layout, and bitmap storage.

use crate::error::{WebError as Error, WebResult};
use num_traits::ToPrimitive;
use pdf_canvas::{CanvasViewport, PageViewport};
use pdf_document::page::PdfPage;
use pdf_graphics::{point::Point, transform::Transform};

/// Validated page viewport using the engine's existing affine and geometry types.
///
/// PdfCanvas already converts page paths to backend device space. The backend
/// applies device-to-backing scaling only. The host applies device-to-CSS to DOM
/// overlays, including any display rotation. CSS coordinates are container-local;
/// the host subtracts the container's client origin before sending pointer input.
#[derive(Clone)]
pub struct WebViewport {
    page: PageViewport,
    canvas: CanvasViewport,
    css_size: [f64; 2],
    device_to_css: Transform,
    revision: u32,
}

impl WebViewport {
    /// Validates positive dimensions and invertible finite mappings.
    /// Page rendering and annotation placement share the supplied page viewport.
    /// Device dimensions are logical engine dimensions, not necessarily bitmap pixels.
    pub fn new(
        page: PageViewport,
        css_size: [f64; 2],
        backing_size: [u32; 2],
        device_to_css: Transform,
        revision: u32,
    ) -> WebResult<Self> {
        if css_size.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            return Err(Error::InvalidInput("CSS dimensions"));
        }
        device_to_css.try_inverse()?;
        let canvas = CanvasViewport::new(page.device_size(), backing_size)?;
        Ok(Self {
            page,
            canvas,
            css_size,
            device_to_css,
            revision,
        })
    }

    /// Lays out `page` at `zoom` CSS pixels per point, `dpr` bitmap pixels per CSS
    /// pixel, and a clockwise display rotation that must be a multiple of 90 degrees.
    /// Page content is rendered unrotated at the page's own size; the rotation lives in
    /// `device_to_css`, so the host applies it to every layer as a CSS transform.
    pub fn for_page(
        page: &PdfPage,
        zoom: f32,
        dpr: f32,
        display_rotation: u32,
        revision: u32,
    ) -> WebResult<Self> {
        if !zoom.is_finite() || zoom <= 0.0 || !dpr.is_finite() || dpr <= 0.0 {
            return Err(Error::InvalidInput("display scale"));
        }
        let [w, h] = PageViewport::page_size(page)?;
        let device_to_css = match display_rotation % 360 {
            0 => Transform::from_scale(zoom, zoom),
            90 => Transform::from_row(0.0, zoom, -zoom, 0.0, h * zoom, 0.0),
            180 => Transform::from_row(-zoom, 0.0, 0.0, -zoom, w * zoom, h * zoom),
            270 => Transform::from_row(0.0, -zoom, zoom, 0.0, 0.0, w * zoom),
            _ => return Err(Error::InvalidInput("display rotation")),
        };
        let css = if display_rotation.is_multiple_of(180) {
            [f64::from(w * zoom), f64::from(h * zoom)]
        } else {
            [f64::from(h * zoom), f64::from(w * zoom)]
        };
        let backing = |v: f32| (v * zoom * dpr).ceil().to_u32().ok_or(Error::ResourceLimit);
        Self::new(
            PageViewport::from_page(page, None, [w, h])?,
            css,
            [backing(w)?, backing(h)?],
            device_to_css,
            revision,
        )
    }

    /// Returns the mapping shared by PDF rendering and annotation geometry.
    pub fn page(&self) -> &PageViewport {
        &self.page
    }

    /// Returns the bitmap viewport consumed by the Canvas 2D backend.
    pub fn canvas(&self) -> &CanvasViewport {
        &self.canvas
    }

    /// Returns the dimensions exposed by CanvasBackend::width and height.
    pub fn device_size(&self) -> [f32; 2] {
        self.page.device_size()
    }

    /// Returns the host-assigned CSS size for the canvas and overlay containers.
    pub fn css_size(&self) -> [f64; 2] {
        self.css_size
    }

    /// Returns actual bitmap dimensions, including high-DPI scaling.
    pub fn backing_size(&self) -> [u32; 2] {
        self.canvas.backing_size()
    }

    /// Returns the page mapping shared with annotation placement; do not reapply to paths.
    pub fn page_to_device(&self) -> &Transform {
        self.page.page_to_device()
    }

    /// Returns the host DOM mapping, independent of temporary content/mask transforms.
    pub fn device_to_css(&self) -> &Transform {
        &self.device_to_css
    }

    /// Returns the validated logical device to bitmap mapping.
    pub fn device_to_backing(&self) -> &Transform {
        self.canvas.device_to_backing()
    }

    /// Maps a container-local CSS pointer into the current engine device coordinates.
    pub fn pointer_to_device(&self, point: Point) -> WebResult<Point> {
        Ok(self.device_to_css.try_inverse()?.try_map_point(point)?)
    }

    /// Converts a CSS pixel displacement into a PDF page displacement for drag input.
    /// Only the linear part of the mapping applies; the origin translation is ignored.
    pub fn css_delta_to_page(&self, dx: f64, dy: f64) -> WebResult<[f64; 2]> {
        let inverse = self
            .device_to_css
            .post_concatenated(self.page_to_device())
            .try_inverse()?;
        let delta = [
            f64::from(inverse.sx) * dx + f64::from(inverse.kx) * dy,
            f64::from(inverse.ky) * dx + f64::from(inverse.sy) * dy,
        ];
        if delta.iter().any(|v| !v.is_finite()) {
            return Err(Error::InvalidInput("drag displacement"));
        }
        Ok(delta)
    }

    /// Identifies this viewport for rejection of delayed framework updates.
    pub fn revision(&self) -> u32 {
        self.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inverse_round_trips_rotated_scaled_points() {
        let t = Transform::from_row(0.0, 2.0, -3.0, 0.0, 80.0, 10.0);
        let p = Point::new(7.0, 11.0);
        let result = t
            .try_inverse()
            .unwrap()
            .try_map_point(t.try_map_point(p).unwrap())
            .unwrap();
        assert!((result.x - p.x).abs() < 0.0001 && (result.y - p.y).abs() < 0.0001);
    }
    #[test]
    fn separates_device_css_and_backing() {
        let viewport = WebViewport::new(
            PageViewport::from_page(
                &pdf_document::page::PdfPage::default(),
                None,
                [100.0, 200.0],
            )
            .unwrap(),
            [150.0, 300.0],
            [300, 600],
            Transform::from_scale(1.5, 1.5),
            7,
        )
        .unwrap();
        assert_eq!(
            *viewport.device_to_backing(),
            Transform::from_scale(3.0, 3.0)
        );
        assert_eq!(
            viewport.pointer_to_device(Point::new(15.0, 30.0)).unwrap(),
            Point::new(10.0, 20.0)
        );
    }
    #[test]
    fn rejects_nonfinite_and_singular_viewports() {
        assert!(
            WebViewport::new(
                PageViewport::from_page(&pdf_document::page::PdfPage::default(), None, [1.0, 1.0])
                    .unwrap(),
                [1.0, 1.0],
                [1, 1],
                Transform::from_scale(0.0, 1.0),
                0
            )
            .is_err()
        );
        assert!(
            PageViewport::from_page(
                &pdf_document::page::PdfPage::default(),
                None,
                [f32::NAN, 1.0]
            )
            .is_err()
        );
    }
}
