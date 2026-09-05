use bytes::Bytes;

use crate::PixelFormat;

/// Represents render-ready raster image pixels.
#[derive(Debug, Clone)]
pub struct Image {
    /// Shared pixel data.
    pub data: Bytes,
    /// The width of the image in pixels.
    pub width: usize,
    /// The height of the image in pixels.
    pub height: usize,
    /// The pixel format of the image data.
    pub pixel_format: PixelFormat,
}

impl Image {
    /// Creates a render-ready image from decoded component samples.
    ///
    /// Single-component images without a soft mask retain their grayscale
    /// buffer. All other images are converted to RGBA pixels.
    pub fn from_decoded_samples(
        data: Bytes,
        width: usize,
        height: usize,
        num_color_components: usize,
        soft_mask: Option<&Self>,
    ) -> Self {
        let convert_to_rgba = soft_mask.is_some() || num_color_components != 1;
        if !convert_to_rgba {
            return Self {
                data,
                width,
                height,
                pixel_format: PixelFormat::Gray8,
            };
        }

        Self {
            data: Bytes::from(Self::to_rgba(
                data.as_ref(),
                width,
                height,
                num_color_components,
                soft_mask,
            )),
            width,
            height,
            pixel_format: PixelFormat::RGBA8888,
        }
    }

    /// Converts decoded image samples into RGBA pixels with an optional soft-mask alpha channel.
    fn to_rgba(
        image_data: &[u8],
        width: usize,
        height: usize,
        num_color_components: usize,
        soft_mask: Option<&Self>,
    ) -> Vec<u8> {
        match num_color_components {
            0 => Vec::new(),
            1 => Self::convert_pixels(
                image_data,
                width,
                height,
                num_color_components,
                soft_mask,
                Self::append_gray_rgba,
            ),
            3 => Self::convert_pixels(
                image_data,
                width,
                height,
                num_color_components,
                soft_mask,
                Self::append_rgb_rgba,
            ),
            4 => Self::convert_pixels(
                image_data,
                width,
                height,
                num_color_components,
                soft_mask,
                Self::append_cmyk_rgba,
            ),
            _ => Self::convert_pixels(
                image_data,
                width,
                height,
                num_color_components,
                soft_mask,
                Self::append_fallback_rgba,
            ),
        }
    }

    /// Converts component chunks with a format-specific pixel writer.
    #[inline]
    fn convert_pixels(
        image_data: &[u8],
        width: usize,
        height: usize,
        num_color_components: usize,
        soft_mask: Option<&Self>,
        mut write_pixel: impl FnMut(&mut [u8], &[u8], u8),
    ) -> Vec<u8> {
        let declared_pixels = width.saturating_mul(height);
        let available_pixels = image_data
            .len()
            .checked_div(num_color_components)
            .map_or(0, std::convert::identity);
        let num_pixels = declared_pixels.min(available_pixels);
        let mut out = vec![0; num_pixels.saturating_mul(4)];
        let mut pixels = out
            .chunks_exact_mut(4)
            .zip(image_data.chunks_exact(num_color_components));

        if let Some(mask) = soft_mask {
            for (alpha, (out_pixel, components)) in mask.data.iter().copied().zip(pixels.by_ref()) {
                write_pixel(out_pixel, components, alpha);
            }
        }

        for (out_pixel, components) in pixels {
            write_pixel(out_pixel, components, 255);
        }

        out
    }

    /// Appends a grayscale sample as an RGBA pixel.
    #[inline]
    fn append_gray_rgba(out: &mut [u8], components: &[u8], alpha: u8) {
        let &[gray] = components else {
            return;
        };
        let [r, g, b, a] = out else {
            return;
        };
        *r = gray;
        *g = gray;
        *b = gray;
        *a = alpha;
    }

    /// Appends an RGB sample as an RGBA pixel.
    #[inline]
    fn append_rgb_rgba(out: &mut [u8], rgb: &[u8], alpha: u8) {
        let &[r, g, b] = rgb else {
            return;
        };
        let [out_r, out_g, out_b, out_a] = out else {
            return;
        };
        *out_r = r;
        *out_g = g;
        *out_b = b;
        *out_a = alpha;
    }

    /// Appends a CMYK sample converted to RGBA.
    #[inline]
    fn append_cmyk_rgba(out: &mut [u8], cmyk: &[u8], alpha: u8) {
        let &[c, m, y, k] = cmyk else {
            return;
        };
        let [r, g, b, a] = out else {
            return;
        };
        *r = Self::cmyk_channel(c, k);
        *g = Self::cmyk_channel(m, k);
        *b = Self::cmyk_channel(y, k);
        *a = alpha;
    }

    /// Converts one CMYK color channel to its RGB equivalent.
    #[inline]
    fn cmyk_channel(component: u8, key: u8) -> u8 {
        let component = 255u16.saturating_sub(u16::from(component));
        let key = 255u16.saturating_sub(u16::from(key));
        let channel = component.saturating_mul(key) / 255;

        u8::try_from(channel).map_or(u8::MAX, std::convert::identity)
    }

    /// Appends a best-effort RGBA pixel for unsupported component counts.
    #[inline]
    fn append_fallback_rgba(out: &mut [u8], components: &[u8], alpha: u8) {
        let [r, g, b, a] = out else {
            return;
        };
        for (channel, component) in [r, g, b].into_iter().zip(components) {
            *channel = *component;
        }
        *a = alpha;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::Image;
    use crate::PixelFormat;
    use bytes::Bytes;

    #[test]
    fn clone_shares_pixel_data() {
        let data = Bytes::from_static(&[1, 2, 3, 4]);
        let image = Image {
            data: data.clone(),
            width: 1,
            height: 1,
            pixel_format: PixelFormat::RGBA8888,
        };

        assert_eq!(image.clone().data.as_ptr(), data.as_ptr());
    }

    #[test]
    fn grayscale_without_soft_mask_reuses_samples() {
        let data = Bytes::from_static(&[12, 34]);
        let image = Image::from_decoded_samples(data.clone(), 2, 1, 1, None);

        assert_eq!(image.data.as_ptr(), data.as_ptr());
        assert_eq!(image.pixel_format, PixelFormat::Gray8);
    }

    #[test]
    fn rgb_samples_are_expanded_to_rgba() {
        let image = Image::from_decoded_samples(vec![10, 20, 30].into(), 1, 1, 3, None);

        assert_eq!(image.pixel_format, PixelFormat::RGBA8888);
        assert_eq!(image.data.as_ref(), &[10, 20, 30, 255]);
    }

    #[test]
    fn cmyk_samples_are_converted_to_rgba() {
        let image = Image::from_decoded_samples(vec![0, 0, 0, 0].into(), 1, 1, 4, None);

        assert_eq!(image.data.as_ref(), &[255, 255, 255, 255]);
    }

    #[test]
    fn uncommon_component_counts_use_available_color_channels() {
        let image = Image::from_decoded_samples(vec![10, 20].into(), 1, 1, 2, None);

        assert_eq!(image.data.as_ref(), &[10, 20, 0, 255]);
    }

    #[test]
    fn soft_mask_supplies_alpha_samples() {
        let soft_mask = Image {
            data: vec![0x10, 0xE0].into(),
            width: 2,
            height: 1,
            pixel_format: PixelFormat::Gray8,
        };
        let image = Image::from_decoded_samples(vec![0x20, 0xC0].into(), 2, 1, 1, Some(&soft_mask));

        assert_eq!(
            image.data.as_ref(),
            &[0x20, 0x20, 0x20, 0x10, 0xC0, 0xC0, 0xC0, 0xE0]
        );
    }

    #[test]
    fn short_soft_mask_defaults_remaining_pixels_to_opaque() {
        let soft_mask = Image {
            data: vec![0x10].into(),
            width: 1,
            height: 1,
            pixel_format: PixelFormat::Gray8,
        };
        let image = Image::from_decoded_samples(vec![0x20, 0xC0].into(), 2, 1, 1, Some(&soft_mask));

        assert_eq!(
            image.data.as_ref(),
            &[0x20, 0x20, 0x20, 0x10, 0xC0, 0xC0, 0xC0, 0xFF]
        );
    }

    #[test]
    fn zero_components_produce_an_empty_rgba_image() {
        let image = Image::from_decoded_samples(vec![10, 20].into(), 1, 1, 0, None);

        assert_eq!(image.pixel_format, PixelFormat::RGBA8888);
        assert!(image.data.is_empty());
    }

    #[test]
    fn incomplete_trailing_components_are_ignored() {
        let image = Image::from_decoded_samples(vec![10, 20, 30, 40, 50].into(), 2, 1, 3, None);

        assert_eq!(image.data.as_ref(), &[10, 20, 30, 255]);
    }

    #[test]
    fn conversion_stops_at_the_declared_pixel_count() {
        let image = Image::from_decoded_samples(vec![10, 20, 30, 40, 50, 60].into(), 1, 1, 3, None);

        assert_eq!(image.data.as_ref(), &[10, 20, 30, 255]);
    }
}
