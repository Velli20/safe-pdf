//! Portable processing of render-ready Gray8, Alpha8 and straight-alpha RGBA images.
//!
//! PDF sample decoding remains separate: four source components may represent
//! CMYK, whereas four-byte pixels here are already converted RGBA. Callers own
//! rendering resources and enforce memory budgets before requesting allocations.

use num_traits::ToPrimitive;
use pdf_graphics::{Image, PixelFormat};

use crate::ImageRasterError;

/// Reserves an empty buffer for exactly `width` × `height` pixels of
/// `bytes_per_pixel` bytes each.
///
/// # Errors
///
/// Returns a resource-limit error if the byte count overflows, or an allocation
/// error if the reservation fails.
pub fn pixel_buffer(
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) -> Result<Vec<u8>, ImageRasterError> {
    let len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .ok_or(ImageRasterError::ResourceLimit)?;
    let mut data = Vec::new();
    data.try_reserve_exact(len)?;
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
/// Returns `None` outside the image's half-open bounds.
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

/// Expands one packed pixel to straight RGBA: Gray8 to opaque gray, Alpha8 to
/// black with its coverage as alpha, and RGBA unchanged.
///
/// Returns `None` if `pixel` is not exactly one pixel long.
fn expand(format: PixelFormat, pixel: &[u8]) -> Option<[u8; 4]> {
    match (format, pixel) {
        (PixelFormat::Gray8, &[gray]) => Some([gray, gray, gray, 255]),
        (PixelFormat::Alpha8, &[alpha]) => Some([0, 0, 0, alpha]),
        (PixelFormat::RGBA8888, &[r, g, b, a]) => Some([r, g, b, a]),
        _ => None,
    }
}

/// Iterates complete pixels as straight RGBA, see [`expand`].
///
/// This iterator borrows the buffer without validating declared dimensions.
/// Decoder soft masks intentionally accept short buffers and ignore incomplete
/// trailing RGBA pixels; public raster operations validate before using it.
pub(crate) fn rgba_pixels(image: &Image) -> impl Iterator<Item = [u8; 4]> + '_ {
    let format = image.pixel_format;
    image
        .data
        .chunks_exact(format.bytes_per_pixel())
        .filter_map(move |pixel| expand(format, pixel))
}

/// Reads the pixel at `index` as straight RGBA, see [`expand`].
///
/// A pixel beyond the end of the buffer reads as zero.
fn pixel_at(image: &Image, index: usize) -> [u8; 4] {
    let format = image.pixel_format;
    let start = index.saturating_mul(format.bytes_per_pixel());
    image
        .data
        .get(start..start.saturating_add(format.bytes_per_pixel()))
        .and_then(|pixel| expand(format, pixel))
        .unwrap_or([0; 4])
}

/// Copies an image into an owned, packed straight-alpha RGBA8 buffer.
///
/// Existing RGBA bytes, including color channels beneath transparent pixels,
/// remain unchanged.
///
/// # Errors
///
/// Returns validation errors from [`Image::validate`] or allocation failures.
pub fn rgba(image: &Image) -> Result<Vec<u8>, ImageRasterError> {
    match image.pixel_format {
        PixelFormat::RGBA8888 => {
            image.validate()?;
            let mut data = pixel_buffer(image.width, image.height, 4)?;
            data.extend_from_slice(&image.data);
            Ok(data)
        }
        PixelFormat::Gray8 | PixelFormat::Alpha8 => {
            Ok(map_pixels(image, |pixel| pixel)?.data.into())
        }
    }
}

/// Builds a same-sized RGBA image by mapping each straight RGBA source pixel.
///
/// Validates before iterating and allocates only the output buffer.
///
/// # Errors
///
/// Returns validation errors from [`Image::validate`] or allocation failures.
pub fn map_pixels(
    image: &Image,
    mut map: impl FnMut([u8; 4]) -> [u8; 4],
) -> Result<Image, ImageRasterError> {
    image.validate()?;
    let mut data = pixel_buffer(image.width, image.height, 4)?;
    for pixel in rgba_pixels(image) {
        data.extend_from_slice(&map(pixel));
    }
    Ok(Image {
        data: data.into(),
        width: image.width,
        height: image.height,
        pixel_format: PixelFormat::RGBA8888,
        interpolate: image.interpolate,
    })
}

/// Maps each pixel's red, green and blue through a transfer function's tables,
/// keeping alpha.
///
/// Gray8 pixels expand to RGB, since each component may map differently.
///
/// # Errors
///
/// Returns validation errors or allocation failures.
pub fn transfer_colors(image: &Image, tables: &[[u8; 256]; 3]) -> Result<Image, ImageRasterError> {
    let [red, green, blue] = tables;
    map_pixels(image, |[r, g, b, a]| {
        [lookup(red, r), lookup(green, g), lookup(blue, b), a]
    })
}

/// Paints a stencil mask's coverage in a solid color.
///
/// Each output pixel takes `color` with the mask's coverage as its alpha, so samples
/// the mask leaves unpainted become transparent.
///
/// # Errors
///
/// Returns validation errors or allocation failures.
pub fn stencil_image(coverage: &Image, [r, g, b]: [u8; 3]) -> Result<Image, ImageRasterError> {
    map_pixels(coverage, |[_, _, _, alpha]| [r, g, b, alpha])
}

/// Looks up a byte in a 256-entry table.
fn lookup(table: &[u8; 256], value: u8) -> u8 {
    table.get(usize::from(value)).copied().unwrap_or(value)
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
/// Returns a resource-limit error if the output byte count overflows, or an
/// allocation error if its reservation fails.
pub(crate) fn resample_with_soft_mask(
    image: &Image,
    mask: &Image,
    width: usize,
    height: usize,
) -> Result<Vec<u8>, ImageRasterError> {
    let mut data = pixel_buffer(width, height, 4)?;
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
            data.extend_from_slice(&[r, g, b, multiply_alpha(alpha, coverage)]);
        }
    }
    Ok(data)
}

/// Averages the soft-mask coverage of the mask pixels at each pair of a row
/// start and a column.
fn average_coverage(mask: &Image, row_starts: [usize; 2], columns: &[usize; 2]) -> u8 {
    let total: u16 = row_starts
        .iter()
        .flat_map(|row_start| {
            columns
                .iter()
                .map(move |column| row_start.saturating_add(*column))
        })
        .map(|index| u16::from(soft_mask_coverage(pixel_at(mask, index))))
        .sum();
    u8::try_from(total / 4).unwrap_or(u8::MAX)
}

/// Returns the coverage a straight RGBA soft-mask pixel gives.
///
/// Expanded Gray8 has alpha 255, so this is its original sample. RGBA masks
/// keep integer truncation rather than luminosity weighting.
pub(crate) fn soft_mask_coverage([gray, _, _, alpha]: [u8; 4]) -> u8 {
    multiply_alpha(gray, alpha)
}

/// Multiplies two 8-bit fractions of 255, truncating.
pub(crate) fn multiply_alpha(a: u8, b: u8) -> u8 {
    u8::try_from(u16::from(a).saturating_mul(u16::from(b)) / 255).unwrap_or(u8::MAX)
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
}
