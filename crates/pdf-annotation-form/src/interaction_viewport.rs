//! Validated conversion between PDF page space and device space.

use pdf_document::page::PdfPage;
use pdf_graphics::{point::Point, rect::Rect, transform::Transform};

/// A validated mapping from PDF user space into a device-space viewport.
#[derive(Clone, Copy, Debug)]
pub struct AnnotationViewport {
    /// Affine transform from page coordinates to device coordinates.
    transform: Transform,
    /// Normalized media-box bounds in page coordinates.
    pub(super) page_bounds: Rect,
    /// Horizontal page units represented by one device unit.
    page_units_per_device_x: f32,
    /// Vertical page units represented by one device unit.
    page_units_per_device_y: f32,
}

impl AnnotationViewport {
    /// Creates a viewport for a page and device-space size.
    pub fn from_page(page: &PdfPage, width: f32, height: f32) -> Option<Self> {
        let media_box = page.media_box.as_ref()?;
        let media_width = media_box.width();
        let media_height = media_box.height();
        if !media_width.is_finite()
            || !media_height.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || media_width <= 0.0
            || media_height <= 0.0
            || width <= 0.0
            || height <= 0.0
        {
            return None;
        }

        let device_units_per_page_x = width / media_width;
        let device_units_per_page_y = height / media_height;
        let page_units_per_device_x = media_width / width;
        let page_units_per_device_y = media_height / height;
        let device_origin_x = -(media_box.left * device_units_per_page_x);
        let device_origin_y = height + media_box.top * device_units_per_page_y;
        if !device_units_per_page_x.is_finite()
            || !device_units_per_page_y.is_finite()
            || !page_units_per_device_x.is_finite()
            || !page_units_per_device_y.is_finite()
            || !device_origin_x.is_finite()
            || !device_origin_y.is_finite()
        {
            return None;
        }

        Some(Self {
            transform: Transform::from_row(
                device_units_per_page_x,
                0.0,
                0.0,
                -device_units_per_page_y,
                device_origin_x,
                device_origin_y,
            ),
            page_bounds: media_box.normalized(),
            page_units_per_device_x,
            page_units_per_device_y,
        })
    }

    /// Maps a valid PDF rectangle into normalized device-space bounds.
    pub fn map_rect(&self, rect: &Rect) -> Option<Rect> {
        let rect = rect.normalized();
        rect.is_valid()
            .then(|| self.transform.map_rect(&rect).normalized())
    }

    /// Maps a finite device-space movement into PDF page-space movement.
    pub(super) fn map_device_delta(&self, delta: Point) -> Option<Point> {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return None;
        }
        let mapped = Point::new(
            delta.x * self.page_units_per_device_x,
            -(delta.y * self.page_units_per_device_y),
        );
        (mapped.x.is_finite() && mapped.y.is_finite()).then_some(mapped)
    }

    /// Converts a page-space height into device units.
    pub(super) fn map_page_height(&self, height: f32) -> Option<f32> {
        let device_height = height / self.page_units_per_device_y;
        (device_height.is_finite() && device_height > 0.0).then_some(device_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_media_boxes_with_nonzero_origins() {
        let page = PdfPage {
            media_box: Some(Rect {
                left: 50.0,
                top: 25.0,
                right: 250.0,
                bottom: 125.0,
            }),
            ..Default::default()
        };
        let viewport =
            AnnotationViewport::from_page(&page, 400.0, 200.0).expect("viewport should be valid");
        let mapped = viewport
            .map_rect(&Rect {
                left: 50.0,
                top: 25.0,
                right: 250.0,
                bottom: 125.0,
            })
            .expect("media box should map");

        assert_eq!(mapped, Rect::new(400.0, 200.0));
    }

    #[test]
    fn rejects_non_finite_derived_scale() {
        let page = PdfPage {
            media_box: Some(Rect {
                left: 0.0,
                top: 0.0,
                right: f32::MAX,
                bottom: f32::MAX,
            }),
            ..Default::default()
        };
        assert!(AnnotationViewport::from_page(&page, f32::MIN_POSITIVE, 1.0).is_none());

        let page = PdfPage {
            media_box: Some(Rect {
                left: 1.0e30,
                top: 0.0,
                right: 1.000_001e30,
                bottom: 100.0,
            }),
            ..Default::default()
        };
        assert!(AnnotationViewport::from_page(&page, 1.0e33, 100.0).is_none());
    }
}
