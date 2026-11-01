//! Decoding a JPEG stream into the samples a PDF image reads.
//!
//! PDF reads the components of a four-component JPEG as CMYK, exactly as the
//! decoder produces them: an Adobe YCCK image is converted to CMYK, but neither
//! form is inverted. Any inversion is the image's `/Decode` array's to apply.
//! Other images are decoded to gray or RGB.

use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
use zune_jpeg::JpegDecoder;

use crate::error::FilterError;

/// The decoded samples of one JPEG image, interleaved per pixel.
pub(crate) struct JpegSamples {
    /// The interleaved samples.
    data: Vec<u8>,
    /// The colour space `data` is in.
    color_space: ColorSpace,
}

impl JpegSamples {
    /// Decodes `jpeg` into gray, RGB, or CMYK samples.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Decompression`] if the data is not a JPEG image
    /// the decoder can read.
    pub(crate) fn decode(jpeg: &[u8]) -> Result<Self, FilterError> {
        let options = DecoderOptions::default()
            .set_strict_mode(false)
            .set_max_width(usize::MAX)
            .set_max_height(usize::MAX);
        let mut decoder = JpegDecoder::new_with_options(ZCursor::new(jpeg), options);
        decoder
            .decode_headers()
            .map_err(|e| FilterError::Decompression(e.to_string()))?;
        let color_space = match decoder.input_colorspace() {
            Some(
                input @ (ColorSpace::Luma
                | ColorSpace::LumaA
                | ColorSpace::RGB
                | ColorSpace::RGBA
                | ColorSpace::CMYK
                | ColorSpace::YCCK),
            ) => input,
            _ => ColorSpace::RGB,
        };
        decoder.set_options(options.jpeg_set_out_colorspace(color_space));
        let data = decoder
            .decode()
            .map_err(|e| FilterError::Decompression(e.to_string()))?;
        Ok(Self { data, color_space })
    }

    /// Returns the samples, with YCCK converted to CMYK.
    pub(crate) fn into_samples(mut self) -> Vec<u8> {
        if self.color_space == ColorSpace::YCCK {
            self.convert_ycck_to_cmyk();
        }
        self.data
    }

    /// Converts YCCK pixels to CMYK in place.
    ///
    /// The YCbCr components hold the complements of cyan, magenta, and yellow,
    /// so they convert to RGB and are complemented; black is kept as stored.
    fn convert_ycck_to_cmyk(&mut self) {
        for pixel in self.data.as_chunks_mut::<4>().0 {
            let [luma, blue_difference, red_difference, _] = pixel.map(i32::from);
            let blue_difference = blue_difference.wrapping_sub(128);
            let red_difference = red_difference.wrapping_sub(128);
            // JFIF YCbCr to RGB, in 16-bit fixed point.
            let red = luma.wrapping_add(fixed_product(91_881, red_difference));
            let green = luma.wrapping_sub(
                fixed_product(22_554, blue_difference)
                    .wrapping_add(fixed_product(46_802, red_difference)),
            );
            let blue = luma.wrapping_add(fixed_product(116_130, blue_difference));
            let [cyan, magenta, yellow, _] = pixel;
            *cyan = complement(red);
            *magenta = complement(green);
            *yellow = complement(blue);
        }
    }
}

/// Multiplies `value` by the 16-bit fixed-point `coefficient`, rounding to the
/// nearest integer.
fn fixed_product(coefficient: i32, value: i32) -> i32 {
    coefficient.wrapping_mul(value).wrapping_add(1 << 15) >> 16
}

/// Returns `255 - value`, with `value` clamped to a sample.
fn complement(value: i32) -> u8 {
    u8::try_from(255_i32.wrapping_sub(value.clamp(0, 255))).unwrap_or(0)
}
