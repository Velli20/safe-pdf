use std::borrow::Cow;

use pdf_content_stream::pdf_operator_backend::XObjectOps;
use pdf_graphics::{ImageEncoding, rect::Rect, transform::Transform};
use pdf_page::{image::ImageFilter, xobject::XObject};

use crate::{canvas_backend::Image, error::PdfCanvasError, pdf_canvas::PdfCanvas};

/// Compose the CTM with an image-space correction transform.
///
/// PDF image XObjects are defined in a normalized unit square ([0,1]×[0,1])
/// where the origin is at the top-left and the Y axis grows downward, while
/// PDF user space has its origin at the bottom-left with the Y axis growing
/// upward. This function post-concatenates the provided Current
/// Transformation Matrix (CTM) with the matrix `[ 1 0 0 -1 0 1 ]`, which
/// flips the Y axis and translates by +1 in Y, so that the unit square maps
/// correctly into user space without vertical inversion.
///
/// # Parameters
///
/// - `ctm`: The current transformation matrix that positions and scales the
///   image on the page.
///
/// # Returns
///
/// A `Transform` that maps the unit square `(0,0,1,1)` into the destination
/// rectangle with the correct orientation for rendering.
fn generate_image_orientation_matrix(mut ctm: Transform) -> Transform {
    const IMAGE_SPACE_Y_FLIP: Transform = Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, 1.0);
    ctm.post_concat(&IMAGE_SPACE_Y_FLIP);
    ctm
}

impl<T: std::error::Error> XObjectOps for PdfCanvas<'_, T> {
    fn invoke_xobject(&mut self, xobject_name: &str) -> Result<(), Self::ErrorType> {
        let resources = self
            .current_state()?
            .resources
            .ok_or(PdfCanvasError::MissingPageResources)?;

        if let Some(XObject::Image(image)) = resources.xobjects.get(xobject_name) {
            let mask = image
                .smask
                .as_ref()
                .map(|m| Cow::Borrowed(m.data.as_slice()));

            let transform = self.current_state()?.transform;
            let rotation_degrees = transform.rotation_degrees();

            // Post-concatenate a unit-square -> user-space orientation fix that
            // flips the Y axis (image space is top-left origin, PDF user space is
            // bottom-left). This keeps positions/scales the same but makes the
            // image render right-side up in user space.
            let transform = generate_image_orientation_matrix(transform);

            // Determine image encoding based on the filter applied.
            let encoding = match &image.filter {
                Some(ImageFilter::DCTDecode) => ImageEncoding::Jpeg,
                Some(ImageFilter::FlateDecode) => ImageEncoding::Uncompressed,
                Some(ImageFilter::Unsupported(other)) => {
                    return Err(PdfCanvasError::NotImplemented(format!(
                        "{} image filter",
                        other
                    )));
                }
                None => ImageEncoding::Uncompressed,
            };

            // Start from the image's normalized unit-square in image space.
            // After applying the CTM (and the Y-flip above), we obtain the
            // destination rectangle in user/device space.
            const UNIT_RECT: Rect = Rect {
                left: 0.0,
                top: 0.0,
                right: 1.0,
                bottom: 1.0,
            };

            // Map the unit square through the transform to compute an axis-aligned
            // bounding rectangle (AABB) that contains the transformed image.
            let mut dest_rect = transform.map_rect(&UNIT_RECT);

            const ROTATION_TOLERANCE_DEGREES: f32 = 1e-3;

            // For right-angle rotations (±90°, ±270°), the mapped rect's
            // width/height are swapped. Preserve the center and swap extents.
            let angle_mod_180 = rotation_degrees.rem_euclid(180.0);
            if (angle_mod_180 - 90.0).abs() <= ROTATION_TOLERANCE_DEGREES {
                let cx = (dest_rect.left + dest_rect.right) * 0.5;
                let cy = (dest_rect.top + dest_rect.bottom) * 0.5;
                let half_w = dest_rect.height() * 0.5;
                let half_h = dest_rect.width() * 0.5;

                dest_rect.left = cx - half_w;
                dest_rect.right = cx + half_w;
                dest_rect.top = cy - half_h;
                dest_rect.bottom = cy + half_h;
            }

            let image = Image {
                data: Cow::Borrowed(image.data.as_slice()),
                width: image.width,
                height: image.height,
                bits_per_component: Some(image.bits_per_component),
                encoding,
                mask,
            };

            let blend_mode = self.current_state()?.blend_mode;
            self.canvas
                .draw_image_rect(&image, blend_mode, dest_rect, Some(rotation_degrees))
                .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
        } else if let Some(XObject::Form(form)) = resources.xobjects.get(xobject_name) {
            self.render_content_stream(
                &form.content_stream.operations,
                form.matrix,
                form.resources.as_ref(),
            )?;
        } else {
            return Err(PdfCanvasError::XObjectNotFound(xobject_name.to_string()));
        }
        Ok(())
    }
}
