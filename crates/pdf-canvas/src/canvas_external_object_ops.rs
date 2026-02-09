//! External object (XObject) rendering operations for PDF canvas.
//!
//! This module handles the rendering of XObjects, which include:
//! - **Image XObjects**: Raster images embedded in PDF documents
//! - **Form XObjects**: Reusable content streams (like vector graphics groups)
//!
//! The main complexity lies in handling the coordinate space transformations
//! between PDF image space (top-left origin, Y down) and PDF user space
//! (bottom-left origin, Y up).

use std::borrow::Cow;

use pdf_content_stream::pdf_operator_backend::XObjectOps;
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_page::{color_space::ColorSpace, image::ImageXObject, xobject::XObject};

use crate::{
    canvas_backend::{CanvasBackend, Image, ImageData},
    error::PdfCanvasError,
    pdf_canvas::PdfCanvas,
};

/// Number of color components in RGB color space.
const RGB_COMPONENTS: usize = 3;

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

/// Bit mask for extracting the low nibble (4 bits) from a byte.
const MASK_4BIT: u8 = 0x0F;
/// Bit mask for extracting 2 bits from a byte.
const MASK_2BIT: u8 = 0x03;
/// Bit mask for extracting 1 bit from a byte.
const MASK_1BIT: u8 = 0x01;

/// Extracts a color index value from packed bit data.
///
/// Indexed color images in PDF can use 1, 2, 4, or 8 bits per component.
/// This function extracts a single index value from the packed byte stream,
/// handling the bit-level addressing required for sub-byte bit depths.
///
/// # Parameters
///
/// - `data`: The raw packed image data bytes.
/// - `bits`: Bits per component (must be 1, 2, 4, or 8).
/// - `bit_pos`: Current bit position in the data stream; advanced by `bits` on success.
///
/// # Returns
///
/// The extracted index value in the range `[0, 2^bits - 1]`.
///
/// # Errors
///
/// - [`PdfCanvasError::InvalidImageData`] if `data` has insufficient bytes.
/// - [`PdfCanvasError::NotImplemented`] if `bits` is not 1, 2, 4, or 8.
///
/// # Bit Packing Layout
///
/// Bits are packed from MSB to LSB within each byte:
///
/// | Depth | Layout per byte                         |
/// |-------|-----------------------------------------|
/// | 8-bit | `[b7..b0]` (entire byte)                |
/// | 4-bit | `[high_nibble \| low_nibble]`           |
/// | 2-bit | `[b7b6 \| b5b4 \| b3b2 \| b1b0]`        |
/// | 1-bit | `[b7 \| b6 \| b5 \| b4 \| b3 \| b2 \| b1 \| b0]` |
fn extract_index(data: &[u8], bits: usize, bit_pos: &mut usize) -> Result<u32, PdfCanvasError> {
    let byte_index = *bit_pos / 8;
    let bit_offset = *bit_pos % 8;

    let byte = *data.get(byte_index).ok_or_else(|| {
        PdfCanvasError::InvalidImageData(format!(
            "Insufficient data for indexed image: need byte at index {byte_index}, but data length is {}",
            data.len()
        ))
    })?;

    let value = match bits {
        8 => u32::from(byte),
        4 => {
            // High nibble (bits 7–4) at offset 0, low nibble (bits 3–0) at offset 4
            let shift = 4_usize.saturating_sub(bit_offset);
            u32::from((byte >> shift) & MASK_4BIT)
        }
        2 => {
            // Extract 2-bit value; valid offsets are 0, 2, 4, 6
            let shift = 6_usize.saturating_sub(bit_offset);
            u32::from((byte >> shift) & MASK_2BIT)
        }
        1 => {
            // Extract single bit; valid offsets are 0–7
            let shift = 7_usize.saturating_sub(bit_offset);
            u32::from((byte >> shift) & MASK_1BIT)
        }
        _ => {
            return Err(PdfCanvasError::NotImplemented(format!(
                "BitsPerComponent {bits} not supported for indexed images"
            )));
        }
    };

    *bit_pos = bit_pos.saturating_add(bits);
    Ok(value)
}

/// Expands indexed color image data to RGB format.
///
/// Indexed (palette-based) images store each pixel as an index into a color
/// lookup table. This function decodes the packed index data and produces
/// an RGB byte stream suitable for rendering.
///
/// # Parameters
///
/// - `indexed_data`: Raw packed pixel indices.
/// - `lookup`: Color lookup table (RGB triplets, 3 bytes per entry).
/// - `width`: Image width in pixels.
/// - `height`: Image height in pixels.
/// - `bits_per_component`: Bit depth of indices (1, 2, 4, or 8).
/// - `hival`: Maximum valid index value (indices are clamped to this).
///
/// # Returns
///
/// - `Ok(Vec<u8>)`: Expanded RGB data (`width * height * 3` bytes).
/// - `Err`: If data is malformed or insufficient.
///
/// # Example
///
/// For a 2x2 image with 4-bit indices and a 16-color palette:
/// ```text
/// Input:  [0x12, 0x34]  (indices: 1, 2, 3, 4)
/// Output: [R1,G1,B1, R2,G2,B2, R3,G3,B3, R4,G4,B4]
/// ```
fn expand_indexed_to_rgb(
    indexed_data: &[u8],
    lookup: &[u8],
    width: usize,
    height: usize,
    bits_per_component: usize,
    hival: u8,
) -> Result<Vec<u8>, PdfCanvasError> {
    let num_pixels = width.saturating_mul(height);
    let mut out = Vec::with_capacity(num_pixels.saturating_mul(RGB_COMPONENTS));
    let mut bit_pos = 0;

    for pixel_idx in 0..num_pixels {
        let index = extract_index(indexed_data, bits_per_component, &mut bit_pos)?;

        // Clamp index to valid palette range
        // Using u32::from for widening conversion, then usize conversion
        let clamped_index = index.min(u32::from(hival));
        // Conversion from u32 to usize: on 32-bit platforms this is identity,
        // on 64-bit platforms this is a widening cast - both are infallible
        #[allow(clippy::as_conversions)]
        let clamped_index_usize = clamped_index as usize;
        let base = clamped_index_usize.saturating_mul(RGB_COMPONENTS);
        let end = base.saturating_add(RGB_COMPONENTS);

        // Safely access lookup table with bounds checking
        let rgb = lookup.get(base..end).ok_or_else(|| {
            PdfCanvasError::InvalidImageData(format!(
                "Palette index {} out of bounds at pixel {} (lookup table size: {})",
                clamped_index_usize,
                pixel_idx,
                lookup.len()
            ))
        })?;

        out.extend_from_slice(rgb);
    }

    Ok(out)
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
            .ok_or(PdfCanvasError::MissingPageResources)?;

        match resources.xobjects.get(xobject_name) {
            Some(XObject::Image(image)) => self.render_image_xobject(image),
            Some(XObject::Form(form)) => self.render_content_stream(
                &form.content_stream.operations,
                form.matrix,
                Some(&form.bbox),
                form.resources.as_ref(),
                None,
            ),
            None => Err(PdfCanvasError::XObjectNotFound(xobject_name.to_string())),
        }
    }
}

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Renders an image XObject to the canvas.
    ///
    /// Handles the complete image rendering pipeline including coordinate
    /// transformation, color space conversion, and backend delegation.
    fn render_image_xobject(&mut self, image: &ImageXObject) -> Result<(), PdfCanvasError> {
        let transform = self.current_state()?.transform;
        let rotation_degrees = transform.rotation_degrees();

        // Post-concatenate a unit-square → user-space orientation fix that
        // flips the Y axis (image space is top-left origin, PDF user space is
        // bottom-left). This keeps positions/scales the same but makes the
        // image render right-side up in user space.
        let transform = generate_image_orientation_matrix(transform);

        // Map the unit square through the transform to compute an axis-aligned
        // bounding rectangle (AABB) that contains the transformed image
        let dest_rect = Self::compute_destination_rect(&transform, rotation_degrees);

        // Expand indexed color data to RGB if applicable
        let image_data = self.resolve_image_data(image)?;

        let rendered_image = Image {
            data: ImageData::from(image_data),
            width: image.width,
            height: image.height,
            pixel_format: image.pixel_format,
        };

        let blend_mode = self.current_state()?.blend_mode;
        self.canvas.draw_image_rect(
            &rendered_image,
            blend_mode,
            dest_rect,
            Some(rotation_degrees),
        )
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

    /// Resolves image data, expanding indexed colors to RGB when necessary.
    ///
    /// For indexed color spaces with an RGB base, this expands the palette
    /// indices to full RGB triplets. Other color spaces pass through unchanged.
    fn resolve_image_data<'a>(
        &self,
        image: &'a ImageXObject,
    ) -> Result<Cow<'a, [u8]>, PdfCanvasError> {
        match image.color_space.as_ref() {
            Some(ColorSpace::Indexed {
                base,
                hival,
                lookup,
            }) if matches!(base.as_ref(), ColorSpace::DeviceRGB) => {
                let expanded = expand_indexed_to_rgb(
                    &image.data,
                    lookup,
                    image.width,
                    image.height,
                    image.bits_per_component,
                    *hival,
                )?;
                Ok(Cow::Owned(expanded))
            }
            _ => Ok(Cow::Borrowed(image.data.as_slice())),
        }
    }
}
