//! Conversion of decoded YCCK JPEG samples into CMYK.

use num_traits::ToPrimitive;

/// Interleaved `[Y, Cb, Cr, K]` samples decoded from a JPEG whose Adobe APP14
/// marker declares the YCCK transform.
pub(crate) struct YcckSamples(pub(crate) Vec<u8>);

impl YcckSamples {
    /// Converts each pixel to `[C, M, Y, K]` in place and returns the samples.
    ///
    /// The colour components are converted from YCbCr to RGB and complemented
    /// into CMY; K passes through unchanged. PDF reads DCTDecode output as
    /// CMYK samples subject to the image's `/Decode`, so no Adobe inversion is
    /// applied here.
    pub(crate) fn into_cmyk(mut self) -> Vec<u8> {
        for [y, cb, cr, _k] in self.0.as_chunks_mut::<4>().0 {
            let luma = f32::from(*y);
            let blue_difference = f32::from(*cb) - 128.0;
            let red_difference = f32::from(*cr) - 128.0;
            let red = luma + 1.402 * red_difference;
            let green = luma - 0.344_136 * blue_difference - 0.714_136 * red_difference;
            let blue = luma + 1.772 * blue_difference;
            *y = complement(red);
            *cb = complement(green);
            *cr = complement(blue);
        }
        self.0
    }
}

/// Returns `255 - value`, rounded and clamped to a byte.
fn complement(value: f32) -> u8 {
    (255.0 - value.round())
        .clamp(0.0, 255.0)
        .to_u8()
        .unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ycck_pixel_converts_to_uninverted_cmyk() {
        let cmyk = YcckSamples(vec![60, 226, 85, 12, 255, 128, 128, 0]).into_cmyk();

        assert_eq!(cmyk, vec![255, 198, 21, 12, 0, 0, 0, 0]);
    }
}
