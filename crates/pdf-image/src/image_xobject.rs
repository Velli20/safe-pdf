use std::sync::Arc;

use pdf_color_space::color_space::ColorSpace;
use pdf_filter::filter::{decode_data_with_resolver, decode_with_resolver};
use pdf_graphics::PixelFormat;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver, stream::StreamObject};

use crate::InlineImage;
use crate::decoded_samples::DecodedSamples;
use crate::error::PdfImageError;
use crate::image_metadata::ImageMetadata;

/// Represents a PDF Image XObject, which is a self-contained raster image.
#[derive(Debug, Clone)]
pub struct ImageXObject {
    /// The width of the image in samples (pixels).
    pub width: usize,
    /// The height of the image in samples (pixels).
    pub height: usize,
    /// The number of bits used to represent each color component.
    pub bits_per_component: usize,
    /// The shared image stream data (with soft mask alpha applied if present).
    pub data: Arc<[u8]>,
    /// The pixel format of the image data.
    pub pixel_format: PixelFormat,
    /// The color space of the image samples.
    pub color_space: Option<ColorSpace>,
}

impl ImageXObject {
    /// Parses an Image XObject from a PDF stream dictionary and data.
    pub fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &dyn ObjectResolver,
        soft_mask: Option<ImageXObject>,
    ) -> Result<Self, PdfImageError> {
        let metadata = ImageMetadata::from_dictionary(dictionary, objects)?;
        match Self::decode_normalized_image_with_metadata(
            dictionary,
            stream_data.raw_data(),
            objects,
            soft_mask.clone(),
            &metadata,
        ) {
            Ok(image) => Ok(image),
            Err(original_error) if metadata.filters.is_some() => {
                let decoded = decode_with_resolver(stream_data, objects)?;
                Self::decode_normalized_image_with_metadata(
                    dictionary,
                    decoded.as_ref(),
                    objects,
                    soft_mask,
                    &metadata,
                )
                .map_err(|_| original_error)
            }
            Err(error) => Err(error),
        }
    }

    /// Decodes an inline image, including its filter chain and normalized sample data.
    pub fn decode_inline_image(
        image: &InlineImage,
        objects: &dyn ObjectResolver,
        soft_mask: Option<ImageXObject>,
    ) -> Result<Self, PdfImageError> {
        let dictionary = image.normalized_dictionary();
        let decoded = decode_data_with_resolver(&dictionary, image.shared_data(), objects)?;

        Self::decode_normalized_image(&dictionary, decoded.as_ref(), objects, soft_mask)
    }

    /// Decodes a normalized image dictionary and raw bytes into a raster image.
    ///
    /// The dictionary must already use canonical image keys such as `Width`,
    /// `Height`, `BitsPerComponent`, and `ColorSpace`.
    pub fn decode_normalized_image(
        dictionary: &Dictionary,
        raw_data: &[u8],
        objects: &dyn ObjectResolver,
        soft_mask: Option<ImageXObject>,
    ) -> Result<Self, PdfImageError> {
        let metadata = ImageMetadata::from_dictionary(dictionary, objects)?;
        Self::decode_normalized_image_with_metadata(
            dictionary, raw_data, objects, soft_mask, &metadata,
        )
    }

    fn decode_normalized_image_with_metadata(
        dictionary: &Dictionary,
        raw_data: &[u8],
        objects: &dyn ObjectResolver,
        soft_mask: Option<ImageXObject>,
        metadata: &ImageMetadata,
    ) -> Result<Self, PdfImageError> {
        let decoded_samples = DecodedSamples::decode(dictionary, raw_data, objects, metadata)?;
        let (data, pixel_format) = Self::assemble_pixel_data(metadata, &decoded_samples, soft_mask);

        Ok(Self {
            width: metadata.size.width(),
            height: metadata.size.height(),
            bits_per_component: decoded_samples.bits_per_component,
            data: data.into(),
            pixel_format,
            color_space: decoded_samples.stored_color_space,
        })
    }

    /// Builds the final pixel buffer and pixel format after optional soft-mask application.
    fn assemble_pixel_data(
        metadata: &ImageMetadata,
        decoded_samples: &DecodedSamples,
        smask: Option<ImageXObject>,
    ) -> (Vec<u8>, PixelFormat) {
        if smask.is_some() || decoded_samples.num_color_components != 1 {
            return (
                Self::to_rgba(
                    &decoded_samples.image_data,
                    metadata.size.width(),
                    metadata.size.height(),
                    decoded_samples.num_color_components,
                    smask.as_ref(),
                ),
                PixelFormat::RGBA8888,
            );
        }

        (decoded_samples.image_data.clone(), PixelFormat::Gray8)
    }

    /// Converts decoded image samples into RGBA pixels with an optional soft-mask alpha channel.
    fn to_rgba(
        image_data: &[u8],
        width: usize,
        height: usize,
        num_color_components: usize,
        smask: Option<&ImageXObject>,
    ) -> Vec<u8> {
        let num_pixels = width.saturating_mul(height);
        let mut out = Vec::with_capacity(num_pixels.saturating_mul(4));

        for (pixel_index, chunk) in image_data
            .chunks_exact(num_color_components)
            .take(num_pixels)
            .enumerate()
        {
            let alpha = Self::alpha_for_pixel(smask, pixel_index);
            match num_color_components {
                4 => Self::append_cmyk_rgba(&mut out, chunk, alpha),
                3 => Self::append_rgb_rgba(&mut out, chunk, alpha),
                1 => Self::append_gray_rgba(&mut out, chunk.first().copied().unwrap_or(0), alpha),
                _ => Self::append_fallback_rgba(&mut out, chunk, alpha),
            }
        }

        out
    }

    /// Returns the alpha value for a pixel from the soft mask, defaulting to full opacity.
    fn alpha_for_pixel(smask: Option<&ImageXObject>, pixel_index: usize) -> u8 {
        smask
            .and_then(|mask| mask.data.get(pixel_index).copied())
            .unwrap_or(255)
    }

    /// Appends a grayscale sample as an RGBA pixel.
    fn append_gray_rgba(out: &mut Vec<u8>, gray: u8, alpha: u8) {
        out.extend_from_slice(&[gray, gray, gray, alpha]);
    }

    /// Appends an RGB sample as an RGBA pixel.
    fn append_rgb_rgba(out: &mut Vec<u8>, rgb: &[u8], alpha: u8) {
        let r = rgb.first().copied().unwrap_or(0);
        let g = rgb.get(1).copied().unwrap_or(0);
        let b = rgb.get(2).copied().unwrap_or(0);
        out.extend_from_slice(&[r, g, b, alpha]);
    }

    /// Appends a CMYK sample converted to RGBA.
    fn append_cmyk_rgba(out: &mut Vec<u8>, cmyk: &[u8], alpha: u8) {
        let c = cmyk.first().copied().unwrap_or(0);
        let m = cmyk.get(1).copied().unwrap_or(0);
        let y = cmyk.get(2).copied().unwrap_or(0);
        let k = cmyk.get(3).copied().unwrap_or(0);

        let c_inv = 255u16.saturating_sub(u16::from(c));
        let m_inv = 255u16.saturating_sub(u16::from(m));
        let y_inv = 255u16.saturating_sub(u16::from(y));
        let k_inv = 255u16.saturating_sub(u16::from(k));

        let r = u8::try_from(c_inv.saturating_mul(k_inv) / 255).unwrap_or(0);
        let g = u8::try_from(m_inv.saturating_mul(k_inv) / 255).unwrap_or(0);
        let b = u8::try_from(y_inv.saturating_mul(k_inv) / 255).unwrap_or(0);
        out.extend_from_slice(&[r, g, b, alpha]);
    }

    /// Appends a best-effort RGBA pixel for unsupported component counts.
    fn append_fallback_rgba(out: &mut Vec<u8>, components: &[u8], alpha: u8) {
        let r = components.first().copied().unwrap_or(0);
        let g = components.get(1).copied().unwrap_or(0);
        let b = components.get(2).copied().unwrap_or(0);
        out.extend_from_slice(&[r, g, b, alpha]);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use pdf_object::{
        dictionary::Dictionary, error::ObjectError, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant,
    };

    use super::{ImageXObject, InlineImage};
    use crate::error::PdfImageError;

    #[test]
    fn cloned_image_xobject_shares_data() {
        let data: Arc<[u8]> = vec![1, 2, 3, 4].into();
        let image = ImageXObject {
            width: 1,
            height: 1,
            bits_per_component: 8,
            data: Arc::clone(&data),
            pixel_format: pdf_graphics::PixelFormat::RGBA8888,
            color_space: None,
        };

        let cloned = image.clone();

        assert!(Arc::ptr_eq(&cloned.data, &data));
    }

    #[test]
    fn decode_normalized_1bpc_gray_inverts_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(1)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (
                "Decode".to_string(),
                ObjectVariant::Array(vec![ObjectVariant::Integer(1), ObjectVariant::Integer(0)]),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(4)),
        ]));

        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[0b1010_0000],
            &PassthroughResolver,
            None,
        )
        .expect("1-bpc decoded grayscale image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(image.data.as_ref(), &[0x00, 0xFF, 0x00, 0xFF]);
    }

    #[test]
    fn decode_normalized_8bpc_gray_remaps_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (
                "Decode".to_string(),
                ObjectVariant::Array(vec![ObjectVariant::Integer(0), ObjectVariant::Real(0.5)]),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(2)),
        ]));

        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[0, 255],
            &PassthroughResolver,
            None,
        )
        .expect("8-bpc decoded grayscale image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(image.data.as_ref(), &[0x00, 0x80]);
    }

    #[test]
    fn decode_normalized_indexed_image_applies_decode_before_lookup() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(1)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Array(vec![
                    ObjectVariant::Name(b"Indexed".to_vec()),
                    ObjectVariant::Name(b"DeviceRGB".to_vec()),
                    ObjectVariant::Integer(1),
                    ObjectVariant::HexString(vec![10, 11, 12, 20, 21, 22]),
                ]),
            ),
            (
                "Decode".to_string(),
                ObjectVariant::Array(vec![ObjectVariant::Integer(1), ObjectVariant::Integer(0)]),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(2)),
        ]));

        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[0b1000_0000],
            &PassthroughResolver,
            None,
        )
        .expect("decoded indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(image.data.as_ref(), &[10, 11, 12, 255, 20, 21, 22, 255]);
    }

    #[test]
    fn decode_normalized_image_rejects_invalid_decode_length() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (
                "Decode".to_string(),
                ObjectVariant::Array(vec![ObjectVariant::Integer(0)]),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(1)),
        ]));

        let err =
            ImageXObject::decode_normalized_image(&dictionary, &[0], &PassthroughResolver, None)
                .expect_err("invalid /Decode length should fail");

        assert!(matches!(
            err,
            PdfImageError::InvalidDecodeLength {
                expected_values: 2,
                actual_values: 1
            }
        ));
    }

    #[test]
    fn decode_normalized_image_without_decode_preserves_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(2)),
        ]));

        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[12, 34],
            &PassthroughResolver,
            None,
        )
        .expect("grayscale image without /Decode should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(image.data.as_ref(), &[12, 34]);
    }

    #[test]
    fn decode_normalized_image_mask_defaults_bits_per_component_to_one() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("ImageMask".to_string(), ObjectVariant::Boolean(true)),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(4)),
        ]));

        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[0b1010_0000],
            &PassthroughResolver,
            None,
        )
        .expect("image masks should default missing BitsPerComponent to 1");

        assert_eq!(image.bits_per_component, 1);
        assert!(image.color_space.is_none());
        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(image.data.as_ref(), &[0x00, 0xFF, 0x00, 0xFF]);
    }

    #[test]
    fn decode_normalized_jpx_image_without_bits_per_component_infers_rgb_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                "Filter".to_string(),
                ObjectVariant::Name(b"JPXDecode".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(2)),
        ]));

        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[1, 2, 3, 4, 5, 6],
            &PassthroughResolver,
            None,
        )
        .expect("JPX images should decode without BitsPerComponent when already expanded");

        assert_eq!(image.bits_per_component, 8);
        assert!(matches!(
            image.color_space,
            Some(pdf_color_space::color_space::ColorSpace::DeviceRGB)
        ));
        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(image.data.as_ref(), &[1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn decode_normalized_non_mask_image_still_requires_bits_per_component() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(1)),
        ]));

        let err =
            ImageXObject::decode_normalized_image(&dictionary, &[0], &PassthroughResolver, None)
                .expect_err("non-mask images should still require BitsPerComponent");

        assert!(matches!(
            err,
            PdfImageError::Object(ObjectError::MissingRequiredKey { ref key }) if key == "BitsPerComponent"
        ));
    }

    #[test]
    fn decode_normalized_dct_cmyk_accepts_preconverted_rgb_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceCMYK".to_vec()),
            ),
            (
                "Filter".to_string(),
                ObjectVariant::Name(b"DCTDecode".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(2)),
        ]));

        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[10, 20, 30, 40, 50, 60],
            &PassthroughResolver,
            None,
        )
        .expect("DCT-decoded RGB bytes should not be validated as CMYK samples");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(image.data.as_ref(), &[10, 20, 30, 255, 40, 50, 60, 255]);
        assert!(matches!(
            image.color_space,
            Some(pdf_color_space::color_space::ColorSpace::DeviceRGB)
        ));
    }

    #[test]
    fn decode_normalized_non_dct_cmyk_still_rejects_rgb_sized_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceCMYK".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(2)),
        ]));

        let err = ImageXObject::decode_normalized_image(
            &dictionary,
            &[10, 20, 30, 40, 50, 60],
            &PassthroughResolver,
            None,
        )
        .expect_err("non-DCT CMYK image data should still require four components");

        assert!(matches!(
            err,
            PdfImageError::TruncatedImageData {
                expected_bytes: 8,
                actual_bytes: 6
            }
        ));
    }

    #[test]
    fn decode_normalized_dct_single_pixel_expands_to_declared_size() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceRGB".to_vec()),
            ),
            (
                "Filter".to_string(),
                ObjectVariant::Name(b"DCTDecode".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(2)),
        ]));

        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[0xAA, 0x10, 0x20],
            &PassthroughResolver,
            None,
        )
        .expect("single decoded DCT pixel should expand to the declared image size");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[0xAA, 0x10, 0x20, 255, 0xAA, 0x10, 0x20, 255]
        );
        assert!(matches!(
            image.color_space,
            Some(pdf_color_space::color_space::ColorSpace::DeviceRGB)
        ));
    }

    #[test]
    fn decode_normalized_image_applies_resolved_soft_mask() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(2)),
        ]));
        let soft_mask = ImageXObject {
            width: 2,
            height: 1,
            bits_per_component: 8,
            data: vec![0x10, 0xE0].into(),
            pixel_format: pdf_graphics::PixelFormat::Gray8,
            color_space: None,
        };

        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[0x20, 0xC0],
            &PassthroughResolver,
            Some(soft_mask),
        )
        .expect("resolved soft mask should be applied");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[0x20, 0x20, 0x20, 0x10, 0xC0, 0xC0, 0xC0, 0xE0]
        );
    }

    #[test]
    fn decode_inline_image_applies_filter_chain_before_samples() {
        let image = InlineImage::new(
            Dictionary::new(BTreeMap::from([
                ("BPC".to_string(), ObjectVariant::Integer(8)),
                (
                    "CS".to_string(),
                    ObjectVariant::Name(b"DeviceGray".to_vec()),
                ),
                (
                    "F".to_string(),
                    ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
                ),
                ("H".to_string(), ObjectVariant::Integer(1)),
                ("W".to_string(), ObjectVariant::Integer(1)),
            ])),
            b"2A>".to_vec(),
        );

        let decoded = ImageXObject::decode_inline_image(&image, &PassthroughResolver, None)
            .expect("inline image should decode");

        assert_eq!(decoded.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(decoded.data.as_ref(), &[0x2A]);
    }

    #[test]
    fn decode_inline_image_accepts_abbreviated_gray_color_space() {
        let image = InlineImage::new(
            Dictionary::new(BTreeMap::from([
                ("BPC".to_string(), ObjectVariant::Integer(1)),
                ("CS".to_string(), ObjectVariant::Name(b"G".to_vec())),
                ("H".to_string(), ObjectVariant::Integer(1)),
                ("W".to_string(), ObjectVariant::Integer(4)),
            ])),
            vec![0b1010_0000],
        );

        let decoded = ImageXObject::decode_inline_image(&image, &PassthroughResolver, None)
            .expect("inline image with abbreviated gray color space should decode");

        assert_eq!(decoded.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(decoded.data.as_ref(), &[0xFF, 0x00, 0xFF, 0x00]);
    }

    #[test]
    fn decode_inline_image_accepts_abbreviated_indexed_color_space() {
        let image = InlineImage::new(
            Dictionary::new(BTreeMap::from([
                ("BPC".to_string(), ObjectVariant::Integer(1)),
                (
                    "CS".to_string(),
                    ObjectVariant::Array(vec![
                        ObjectVariant::Name(b"I".to_vec()),
                        ObjectVariant::Name(b"RGB".to_vec()),
                        ObjectVariant::Integer(1),
                        ObjectVariant::HexString(vec![10, 11, 12, 20, 21, 22]),
                    ]),
                ),
                ("H".to_string(), ObjectVariant::Integer(1)),
                ("W".to_string(), ObjectVariant::Integer(4)),
            ])),
            vec![0b1010_0000],
        );

        let decoded = ImageXObject::decode_inline_image(&image, &PassthroughResolver, None)
            .expect("inline image with abbreviated indexed color space should decode");

        assert_eq!(decoded.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            decoded.data.as_ref(),
            &[
                20, 21, 22, 255, 10, 11, 12, 255, 20, 21, 22, 255, 10, 11, 12, 255
            ]
        );
    }

    #[test]
    fn decode_normalized_1bpc_image_expands_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(1)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(8)),
        ]));

        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[0b1011_0010],
            &PassthroughResolver,
            None,
        )
        .expect("1-bpc image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(
            image.data.as_ref(),
            &[0xFF, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0x00]
        );
    }

    #[test]
    fn decode_normalized_indexed_image_1bpc_expands_samples() {
        let dictionary = indexed_dictionary(1);
        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[0b1010_0000],
            &PassthroughResolver,
            None,
        )
        .expect("1-bpc indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[
                20, 21, 22, 255, 10, 11, 12, 255, 20, 21, 22, 255, 10, 11, 12, 255
            ]
        );
    }

    #[test]
    fn decode_normalized_indexed_image_2bpc_expands_samples() {
        let dictionary = indexed_dictionary(2);
        let image =
            ImageXObject::decode_normalized_image(&dictionary, &[0x1B], &PassthroughResolver, None)
                .expect("2-bpc indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[
                10, 11, 12, 255, 20, 21, 22, 255, 30, 31, 32, 255, 40, 41, 42, 255,
            ]
        );
    }

    #[test]
    fn decode_normalized_indexed_image_4bpc_expands_samples() {
        let dictionary = indexed_dictionary(4);
        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[0x01, 0x23],
            &PassthroughResolver,
            None,
        )
        .expect("4-bpc indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[
                10, 11, 12, 255, 20, 21, 22, 255, 30, 31, 32, 255, 40, 41, 42, 255
            ]
        );
    }

    #[test]
    fn decode_normalized_indexed_image_8bpc_continues_to_work() {
        let dictionary = indexed_dictionary(8);
        let image = ImageXObject::decode_normalized_image(
            &dictionary,
            &[0, 1, 2, 3],
            &PassthroughResolver,
            None,
        )
        .expect("8-bpc indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[
                10, 11, 12, 255, 20, 21, 22, 255, 30, 31, 32, 255, 40, 41, 42, 255
            ]
        );
    }

    fn indexed_dictionary(bits_per_component: i64) -> Dictionary {
        Dictionary::new(BTreeMap::from([
            (
                "BitsPerComponent".to_string(),
                ObjectVariant::Integer(bits_per_component),
            ),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Array(vec![
                    ObjectVariant::Name(b"Indexed".to_vec()),
                    ObjectVariant::Name(b"DeviceRGB".to_vec()),
                    ObjectVariant::Integer(3),
                    ObjectVariant::HexString(vec![10, 11, 12, 20, 21, 22, 30, 31, 32, 40, 41, 42]),
                ]),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(4)),
        ]))
    }
}
