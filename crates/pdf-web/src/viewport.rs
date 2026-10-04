//! Shared mappings between existing device geometry, browser layout, and bitmap storage.

use crate::error::{WebError as Error, WebResult};
use num_traits::ToPrimitive;
use pdf_canvas::{CanvasViewport, PageViewport, ViewportError};
use pdf_document::page::PdfPage;
use pdf_graphics::{point::Point, size::Size, transform::Transform, viewport::pixel_extent};

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
    device_to_css: Transform,
    css_to_device: Transform,
    revision: u32,
}

impl WebViewport {
    /// Validates positive dimensions and invertible finite mappings.
    /// Page rendering and annotation placement share the supplied page viewport.
    /// Device dimensions are logical engine dimensions, not necessarily bitmap pixels.
    pub fn new(
        page: PageViewport,
        backing_size: Size<u32>,
        device_to_css: Transform,
        revision: u32,
    ) -> WebResult<Self> {
        let css_to_device = device_to_css.try_inverse()?;
        let canvas = CanvasViewport::new(page.device_size(), backing_size)?;
        Ok(Self {
            page,
            canvas,
            device_to_css,
            css_to_device,
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
        let size = page.page_size().ok_or(ViewportError::Bounds)?;
        let rotation = i32::try_from(display_rotation % 360)
            .map_err(|_| Error::InvalidInput("display rotation"))?;
        let turn = Transform::from_quarter_turn(rotation, size.quarter_turned(rotation))
            .ok_or(Error::InvalidInput("display rotation"))?;
        let device_to_css = Transform::from_scale(zoom, zoom).post_concatenated(&turn);
        let backing = |v: f32| pixel_extent(v * zoom * dpr).ok_or(Error::ResourceLimit);
        Self::new(
            page.viewport(None, size)?,
            Size::new(backing(size.width)?, backing(size.height)?),
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

    /// Returns the host DOM mapping, independent of temporary content/mask transforms.
    pub fn device_to_css(&self) -> &Transform {
        &self.device_to_css
    }

    /// Maps a container-local CSS pointer into the current engine device coordinates.
    pub fn pointer_to_device(&self, point: Point) -> WebResult<Point> {
        Ok(self.css_to_device.try_map_point(point)?)
    }

    /// Converts a CSS pixel displacement into a PDF page displacement for drag input.
    /// Only the linear part of the mapping applies; the origin translation is ignored.
    pub fn css_delta_to_page(&self, dx: f64, dy: f64) -> WebResult<[f64; 2]> {
        let (Some(dx), Some(dy)) = (dx.to_f32(), dy.to_f32()) else {
            return Err(Error::InvalidInput("drag displacement"));
        };
        let device = self
            .css_to_device
            .linear()
            .try_map_point(Point::new(dx, dy))?;
        let page = self.page.map_device_delta(device)?;
        Ok([f64::from(page.x), f64::from(page.y)])
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
    fn separates_device_css_and_backing() {
        let viewport = WebViewport::new(
            pdf_document::page::PdfPage::default()
                .viewport(None, Size::new(100.0, 200.0))
                .unwrap(),
            Size::new(300, 600),
            Transform::from_scale(1.5, 1.5),
            7,
        )
        .unwrap();
        assert_eq!(
            *viewport.canvas().device_to_backing(),
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
                pdf_document::page::PdfPage::default()
                    .viewport(None, Size::new(1.0, 1.0))
                    .unwrap(),
                Size::new(1, 1),
                Transform::from_scale(0.0, 1.0),
                0
            )
            .is_err()
        );
        assert!(
            pdf_document::page::PdfPage::default()
                .viewport(None, Size::new(f32::NAN, 1.0))
                .is_err()
        );
    }
}
