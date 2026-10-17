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

/// Reserves an empty buffer for exactly `width` × `height` packed RGBA8 pixels.
///
/// # Errors
///
/// Returns a resource-limit error if the byte count overflows or the allocation fails.
pub fn rgba_buffer(width: usize, height: usize) -> Result<Vec<u8>, ImageRasterError> {
    let bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(ImageRasterError::ResourceLimit)?;
    let mut data = Vec::new();
    data.try_reserve_exact(bytes)?;
    Ok(data)
}

/// Returns the centers of `count` equal pixel spans across `start..start + extent`.
///
/// The intermediate arithmetic uses `f64` so that centers stay accurate for large
/// counts before being narrowed to the `f32` coordinates transforms operate on.
///
/// # Errors
///
/// Returns an invalid-input error if a center cannot be represented as `f32`.
pub fn pixel_centers(start: f32, extent: f32, count: u32) -> Result<Vec<f32>, ImageRasterError> {
    let (start, extent, len) = (f64::from(start), f64::from(extent), f64::from(count));
    (0..count)
        .map(|index| {
            (start + (f64::from(index) + 0.5) * extent / len)
                .to_f32()
                .ok_or(ImageRasterError::InvalidInput("pixel center"))
        })
        .collect()
}

/// Reads the pixel containing `(x, y)` in source-pixel coordinates as straight RGBA.
///
/// Returns `None` outside the image's half-open bounds. Gray8 expands to opaque RGB.
pub fn nearest_pixel(image: &Image, x: f32, y: f32) -> Option<[u8; 4]> {
    let column = x
        .floor()
        .to_usize()
        .filter(|column| *column < image.width)?;
    let row = y.floor().to_usize().filter(|row| *row < image.height)?;
    Some(pixel_at(
        image,
        row.checked_mul(image.width)?.checked_add(column)?,
    ))
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
    let mut data = rgba_buffer(image.width, image.height)?;
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

/// Combines an image with a soft mask of a different size into `width` ×
/// `height` straight RGBA pixels, in one pass.
///
/// Each output pixel takes its color from the image pixel it falls in and
/// multiplies that pixel's alpha by the mask coverage averaged over a 2 × 2
/// grid of samples spread across the mask area it covers. Enlarging thus
/// replicates pixels, while shrinking a mask keeps detail finer than the
/// output, such as thin strokes, as partial coverage, at a cost bounded by the
/// output size however large the mask is. Unlike the public operations this
/// accepts the short buffers decoders may produce: pixels missing from either
/// source read as zero.
///
/// # Errors
///
/// Returns a resource-limit error if the output byte count overflows or its
/// allocation fails.
pub(crate) fn resample_with_soft_mask(
    image: &Image,
    mask: &Image,
    width: usize,
    height: usize,
) -> Result<Vec<u8>, ImageRasterError> {
    let mut data = rgba_buffer(width, height)?;
    let columns: Vec<_> = (0..width)
        .map(|x| {
            let [image_column, _] = sample_positions(x, image.width, width);
            (image_column, sample_positions(x, mask.width, width))
        })
        .collect();
    for y in 0..height {
        let [image_row, _] = sample_positions(y, image.height, height);
        let image_row_start = image_row.saturating_mul(image.width);
        let mask_row_starts =
            sample_positions(y, mask.height, height).map(|row| row.saturating_mul(mask.width));
        for (image_column, mask_columns) in &columns {
            let [r, g, b, alpha] = pixel_at(image, image_row_start.saturating_add(*image_column));
            let coverage = average_coverage(mask, mask_row_starts, mask_columns);
            let alpha = u16::from(alpha).saturating_mul(u16::from(coverage)) / 255;
            data.extend_from_slice(&[r, g, b, u8::try_from(alpha).unwrap_or(u8::MAX)]);
        }
    }
    Ok(data)
}

/// Averages the soft-mask coverage of the mask pixels at each pair of a row
/// start and a column.
fn average_coverage(mask: &Image, row_starts: [usize; 2], columns: &[usize; 2]) -> u8 {
    let mut total = 0_u16;
    for row_start in row_starts {
        for &column in columns {
            let coverage = soft_mask_coverage(pixel_at(mask, row_start.saturating_add(column)));
            total = total.saturating_add(u16::from(coverage));
        }
    }
    u8::try_from(total / 4).unwrap_or(u8::MAX)
}

/// Returns the coverage a straight RGBA soft-mask pixel gives.
///
/// Expanded Gray8 has alpha 255, so this is its original sample. RGBA masks
/// keep integer truncation rather than luminosity weighting.
pub(crate) fn soft_mask_coverage([gray, _, _, alpha]: [u8; 4]) -> u8 {
    let coverage = u16::from(gray).saturating_mul(u16::from(alpha)) / 255;
    u8::try_from(coverage).unwrap_or(u8::MAX)
}

/// Returns two source rows or columns spread over the span that output row or
/// column `index` covers when `source_len` of them are scaled to `len`.
///
/// A span of one gives that one twice.
fn sample_positions(index: usize, source_len: usize, len: usize) -> [usize; 2] {
    let scale = |index: usize| {
        index
            .saturating_mul(source_len)
            .checked_div(len)
            .unwrap_or(0)
    };
    let start = scale(index);
    let span = scale(index.saturating_add(1)).saturating_sub(start);
    [span / 4, span.saturating_mul(3) / 4].map(|offset| start.saturating_add(offset))
}

/// Reads one pixel as straight RGBA, expanding Gray8 to opaque RGB.
///
/// A pixel beyond the end of the buffer reads as zero.
fn pixel_at(image: &Image, index: usize) -> [u8; 4] {
    match image.pixel_format {
        PixelFormat::Gray8 => image
            .data
            .get(index)
            .map_or([0; 4], |&gray| [gray, gray, gray, 255]),
        PixelFormat::RGBA8888 => {
            let start = index.saturating_mul(4);
            image
                .data
                .get(start..start.saturating_add(4))
                .and_then(|pixel| <[u8; 4]>::try_from(pixel).ok())
                .unwrap_or([0; 4])
        }
    }
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
    let mut data = rgba_buffer(image.width, image.height)?;
    for pixel in rgba_pixels(image) {
        data.extend_from_slice(&[255, 255, 255, alpha(pixel)?]);
    }
    Ok(Image {
        width: image.width,
        height: image.height,
        pixel_format: PixelFormat::RGBA8888,
        interpolate: image.interpolate,
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
            interpolate: false,
            data: vec![0, 127].into(),
        };
        assert_eq!(rgba(&gray).unwrap(), vec![0, 0, 0, 255, 127, 127, 127, 255]);
        let color = Image {
            width: 1,
            height: 1,
            pixel_format: PixelFormat::RGBA8888,
            interpolate: false,
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
            interpolate: false,
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
            interpolate: false,
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
            interpolate: false,
        };
        let mask = luminosity_mask(&image).unwrap();
        assert_eq!(mask.pixel_format, PixelFormat::RGBA8888);
        validate_image(&mask).unwrap();
        assert_eq!([mask.width, mask.height], [2, 1]);
        assert_eq!(mask.data.as_ref(), &[255, 255, 255, 0, 255, 255, 255, 255]);
    }
}
