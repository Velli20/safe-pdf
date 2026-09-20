//! Portable processing of render-ready Gray8 and straight-alpha RGBA images.
//!
//! PDF sample decoding remains separate: four source components may represent
//! CMYK, whereas four-byte pixels here are already converted RGBA. Callers own
//! rendering resources and enforce memory budgets before requesting allocations.

use num_traits::ToPrimitive;
use pdf_graphics::{Image, PixelFormat};

use crate::ImageRasterError;

/// Validates dimensions and the exact packed byte length of a render-ready image.
///
/// Unlike decoded PDF samples, render-ready buffers must not contain trailing
/// bytes. Gray8 images must also have a representable RGBA output byte count.
///
/// # Errors
///
/// Returns an invalid-input error for empty dimensions or a mismatched buffer
/// length, or a resource-limit error if the RGBA byte count overflows.
pub fn validate_image(image: &Image) -> Result<(), ImageRasterError> {
    if image.width == 0 || image.height == 0 {
        return Err(ImageRasterError::InvalidInput("image dimensions"));
    }
    let bytes = image
        .rgba_byte_size()
        .ok_or(ImageRasterError::ResourceLimit)?;
    let expected = match image.pixel_format {
        PixelFormat::RGBA8888 => bytes,
        PixelFormat::Gray8 => bytes / 4,
    };
    if image.data.len() != expected {
        return Err(ImageRasterError::InvalidInput("image byte length"));
    }
    Ok(())
}

/// Iterates complete pixels as straight RGBA, expanding Gray8 with opaque alpha.
///
/// This iterator borrows the buffer without validating declared dimensions.
/// Decoder soft masks intentionally accept short buffers and ignore incomplete
/// trailing RGBA pixels; public raster operations validate before using it.
pub(crate) fn rgba_pixels(image: &Image) -> impl Iterator<Item = [u8; 4]> + '_ {
    let stride = match image.pixel_format {
        PixelFormat::Gray8 => 1,
        PixelFormat::RGBA8888 => 4,
    };
    image
        .data
        .chunks_exact(stride)
        .filter_map(|pixel| match pixel {
            [gray] => Some([*gray, *gray, *gray, 255]),
            [r, g, b, a] => Some([*r, *g, *b, *a]),
            _ => None,
        })
}

/// Copies an image into an owned, packed straight-alpha RGBA8 buffer.
///
/// Gray8 pixels expand to opaque RGB. Existing RGBA bytes, including color
/// channels beneath transparent pixels, remain unchanged.
///
/// # Errors
///
/// Returns validation errors from [`validate_image`] or a resource-limit error
/// if the output allocation fails.
pub fn rgba(image: &Image) -> Result<Vec<u8>, ImageRasterError> {
    validate_image(image)?;
    let mut data = Vec::new();
    data.try_reserve_exact(
        image
            .rgba_byte_size()
            .ok_or(ImageRasterError::ResourceLimit)?,
    )
    .map_err(|_| ImageRasterError::ResourceLimit)?;
    match image.pixel_format {
        PixelFormat::RGBA8888 => data.extend_from_slice(&image.data),
        PixelFormat::Gray8 => {
            for pixel in rgba_pixels(image) {
                data.extend_from_slice(&pixel);
            }
        }
    }
    Ok(data)
}

/// Creates a white RGBA mask whose alpha is the source RGB luminosity.
///
/// Uses straight RGB independently of source alpha, expanding Gray8 first.
/// The weighted `f32` result is clamped and truncated to a byte.
///
/// # Errors
///
/// Returns validation errors, a resource-limit error on allocation failure, or
/// an invalid-input error if luminosity cannot be converted to a byte.
pub fn luminosity_mask(image: &Image) -> Result<Image, ImageRasterError> {
    white_alpha_mask(image, |[r, g, b, _]| {
        // Preserve operation order and truncation, even for expanded gray:
        // simplifying to the gray byte or rounding can change the resulting alpha.
        // Input is straight RGB; multiplying by source alpha would change the mask.
        (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b))
            .clamp(0.0, 255.0)
            .to_u8()
            .ok_or(ImageRasterError::InvalidInput("luminosity"))
    })
}

/// Creates a white mask by mapping source alpha through a soft-mask transfer table.
///
/// # Errors
/// Returns image validation errors or allocation failures.
pub fn transfer_mask(image: &Image, transfer: &[u8; 256]) -> Result<Image, ImageRasterError> {
    white_alpha_mask(image, |[_, _, _, alpha]| {
        transfer
            .get(usize::from(alpha))
            .copied()
            .ok_or(ImageRasterError::InvalidInput("mask transfer"))
    })
}

/// Builds a same-sized white RGBA image with alpha computed from each source pixel.
///
/// Validates before iterating and allocates only the final output buffer.
/// Validation, allocation, and alpha-calculation failures propagate to the caller.
fn white_alpha_mask(
    image: &Image,
    mut alpha: impl FnMut([u8; 4]) -> Result<u8, ImageRasterError>,
) -> Result<Image, ImageRasterError> {
    validate_image(image)?;
    let mut data = Vec::new();
    data.try_reserve_exact(
        image
            .rgba_byte_size()
            .ok_or(ImageRasterError::ResourceLimit)?,
    )
    .map_err(|_| ImageRasterError::ResourceLimit)?;
    for pixel in rgba_pixels(image) {
        data.extend_from_slice(&[255, 255, 255, alpha(pixel)?]);
    }
    Ok(Image {
        width: image.width,
        height: image.height,
        pixel_format: PixelFormat::RGBA8888,
        data: data.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Checks grayscale expansion and preservation of straight RGBA bytes.
    #[test]
    fn expands_gray_and_preserves_straight_alpha() {
        let gray = Image {
            width: 2,
            height: 1,
            pixel_format: PixelFormat::Gray8,
            data: vec![0, 127].into(),
        };
        assert_eq!(rgba(&gray).unwrap(), vec![0, 0, 0, 255, 127, 127, 127, 255]);
        let color = Image {
            width: 1,
            height: 1,
            pixel_format: PixelFormat::RGBA8888,
            data: vec![255, 20, 0, 3].into(),
        };
        assert_eq!(rgba(&color).unwrap(), vec![255, 20, 0, 3]);
    }
    /// Checks rejection of a render-ready image with missing pixel bytes.
    #[test]
    fn rejects_truncated_images() {
        let image = Image {
            width: 1,
            height: 1,
            pixel_format: PixelFormat::RGBA8888,
            data: vec![0].into(),
        };
        assert!(rgba(&image).is_err());
    }
    /// Checks that luminosity ignores the source alpha channel.
    #[test]
    fn luminosity_uses_straight_rgb() {
        let image = Image {
            width: 1,
            height: 1,
            data: vec![255, 0, 0, 64].into(),
            pixel_format: PixelFormat::RGBA8888,
        };
        assert_eq!(
            luminosity_mask(&image).unwrap().data.as_ref(),
            &[255, 255, 255, 76]
        );
    }
    /// Checks grayscale luminosity output format, dimensions, and pixels.
    #[test]
    fn luminosity_expands_gray_to_rgba() {
        let image = Image {
            width: 2,
            height: 1,
            data: vec![0, 255].into(),
            pixel_format: PixelFormat::Gray8,
        };
        let mask = luminosity_mask(&image).unwrap();
        assert_eq!(mask.pixel_format, PixelFormat::RGBA8888);
        validate_image(&mask).unwrap();
        assert_eq!([mask.width, mask.height], [2, 1]);
        assert_eq!(mask.data.as_ref(), &[255, 255, 255, 0, 255, 255, 255, 255]);
    }
}
