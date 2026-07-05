//! External object (XObject) rendering operations for PDF canvas.
//!
//! This module handles the rendering of XObjects, which include:
//! - **Image XObjects**: Raster images embedded in PDF documents
//! - **Form XObjects**: Reusable content streams (like vector graphics groups)
//!
//! The main complexity lies in handling the coordinate space transformations
//! between PDF image space (top-left origin, Y down) and PDF user space
//! (bottom-left origin, Y up).

use pdf_content_stream_operators::pdf_operator_backend::XObjectOps;
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_image::{ImageXObject, InlineImage};
use pdf_object::object_resolver::PassthroughResolver;
use pdf_resources::xobject::XObject;

use crate::{
    canvas_backend::{CanvasBackend, Image, ImageData},
    error::PdfCanvasError,
    pdf_canvas::PdfCanvas,
};

/// Tolerance in degrees for detecting right-angle rotations.
const ROTATION_TOLERANCE_DEGREES: f32 = 1e-3;

/// Transformation matrix that flips the Y axis for image-space to user-space conversion.
///
/// This matrix `[ 1 0 0 -1 0 1 ]` performs:
/// - Scale Y by -1 (flip vertically)
/// - Translate Y by +1 (move origin from top-left to bottom-left)
const IMAGE_SPACE_Y_FLIP: Transform = Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, 1.0);

/// Compose the CTM with an image-space correction transform.
///
/// PDF image XObjects are defined in a normalized unit square (`[0,1] × [0,1]`)
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
/// A [`Transform`] that maps the unit square `(0,0,1,1)` into the destination
/// rectangle with the correct orientation for rendering.
fn generate_image_orientation_matrix(mut ctm: Transform) -> Transform {
    ctm.post_concat(&IMAGE_SPACE_Y_FLIP);
    ctm
}

impl<B: CanvasBackend> XObjectOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    /// Invokes (renders) an XObject by name from the current resource dictionary.
    ///
    /// This method handles two types of XObjects:
    ///
    /// ## Image XObjects
    ///
    /// Raster images are rendered by:
    /// 1. Extracting image metadata (dimensions, color space, encoding)
    /// 2. Computing the destination rectangle via CTM transformation
    /// 3. Handling coordinate space conversion (image space → user space)
    /// 4. Expanding indexed colors to RGB if necessary
    /// 5. Delegating actual drawing to the canvas backend
    ///
    /// ## Form XObjects
    ///
    /// Form XObjects are self-contained content streams that can include
    /// their own resources. They are rendered by recursively processing
    /// the form's content stream with the form's transformation matrix.
    fn invoke_xobject(&mut self, xobject_name: &str) -> Result<(), Self::ErrorType> {
        let resources = self
            .current_state()?
            .resources
            .ok_or(PdfCanvasError::PageResourcesMissing)?;

        let xobj = resources
            .xobject(xobject_name)
            .ok_or_else(|| PdfCanvasError::XObjectNotFound(xobject_name.to_string()))?;

        match xobj {
            XObject::Image(image) => self.render_image_xobject(image)?,
            XObject::Form(form) => self.render_content_stream(
                &form.content_stream,
                form.matrix,
                Some(&form.bbox),
                form.resources.as_ref(),
                None,
            )?,
        }

        Ok(())
    }

    fn paint_inline_image(&mut self, image: &InlineImage) -> Result<(), Self::ErrorType> {
        let decoded = ImageXObject::decode_inline_image(image, &PassthroughResolver, None)
            .map_err(|e| PdfCanvasError::InvalidImageData(e.to_string()))?;

        self.render_decoded_image(&decoded, true)
    }
}

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Renders an image XObject to the canvas.
    pub(crate) fn render_image_xobject(
        &mut self,
        image: &ImageXObject,
    ) -> Result<(), PdfCanvasError> {
        self.render_decoded_image(image, false)
    }

    fn render_decoded_image(
        &mut self,
        image: &ImageXObject,
        inline_image: bool,
    ) -> Result<(), PdfCanvasError> {
        let transform = self.current_state()?.transform;
        let rotation_degrees = transform.rotation_degrees();
        let transform = generate_image_orientation_matrix(transform);
        let dest_rect = Self::compute_destination_rect(&transform, rotation_degrees);

        let rendered_image = Image {
            data: ImageData::Owned(image.data.clone()),
            width: image.width,
            height: image.height,
            pixel_format: image.pixel_format,
        };

        let blend_mode = self.current_state()?.blend_mode;
        if inline_image {
            self.canvas.draw_inline_image(
                &rendered_image,
                blend_mode,
                dest_rect,
                Some(rotation_degrees),
            )
        } else {
            self.canvas.draw_image_rect(
                &rendered_image,
                blend_mode,
                dest_rect,
                Some(rotation_degrees),
            )
        }
    }

    /// Computes the destination rectangle for image rendering.
    ///
    /// Maps the normalized unit square through the transform and adjusts
    /// for right-angle rotations where width/height need to be swapped.
    fn compute_destination_rect(transform: &Transform, rotation_degrees: f32) -> Rect {
        let mut dest_rect = transform.map_rect(&Rect::UNIT_RECT);

        // For right-angle rotations (±90°, ±270°), the mapped rect's
        // width/height are swapped. Preserve the center and swap extents.
        let angle_mod_180 = rotation_degrees.rem_euclid(180.0);
        if (angle_mod_180 - 90.0).abs() <= ROTATION_TOLERANCE_DEGREES {
            let center_x = (dest_rect.left + dest_rect.right) * 0.5;
            let center_y = (dest_rect.top + dest_rect.bottom) * 0.5;
            let half_width = dest_rect.height() * 0.5;
            let half_height = dest_rect.width() * 0.5;

            dest_rect.left = center_x - half_width;
            dest_rect.right = center_x + half_width;
            dest_rect.top = center_y - half_height;
            dest_rect.bottom = center_y + half_height;
        }

        dest_rect
    }
}
