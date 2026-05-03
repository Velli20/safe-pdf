use pdf_color_space::{color_space::ColorSpace, indexed_color_space::IndexedColorSpace};
use pdf_filter::filter::decode_with_resolver;
use pdf_graphics::PixelFormat;
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};

use crate::InlineImage;
use crate::decode::ImageDecode;
use crate::error::PdfImageError;
use crate::indexed::{expand_indexed_values_to_components, unpack_image_samples};

/// Resolves a stream as an image soft mask, or reports a cycle/non-image failure.
pub trait SoftMaskResolver {
    fn resolve_soft_mask(
        &mut self,
        stream: &StreamObject,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<ImageXObject>, PdfImageError>;
}

/// Represents a PDF Image XObject, which is a self-contained raster image.
#[derive(Debug, Clone)]
pub struct ImageXObject {
    /// The width of the image in samples (pixels).
    pub width: usize,
    /// The height of the image in samples (pixels).
    pub height: usize,
    /// The number of bits used to represent each color component.
    pub bits_per_component: usize,
    /// The raw image stream data (with soft mask alpha applied if present).
    pub data: Vec<u8>,
    /// The pixel format of the image data.
    pub pixel_format: PixelFormat,
    /// The color space of the image samples.
    pub color_space: Option<ColorSpace>,
}

/// Stores the normalized metadata needed to decode an image stream.
#[derive(Debug, Clone)]
struct ImageMetadata {
    width: usize,
    height: usize,
    bits_per_component: usize,
    color_space: Option<ColorSpace>,
    image_mask: bool,
}

/// Stores decoded sample bytes before the final pixel format conversion.
#[derive(Debug, Clone)]
struct DecodedSamples {
    stored_color_space: Option<ColorSpace>,
    num_color_components: usize,
    image_data: Vec<u8>,
}

impl ImageXObject {
    /// Parses an Image XObject from a PDF stream dictionary and data.
    pub fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &dyn ObjectResolver,
        soft_mask_resolver: &mut dyn SoftMaskResolver,
    ) -> Result<Self, PdfImageError> {
        let raw_data = stream_data.data()?;
        Self::decode_normalized_image(
            dictionary,
            raw_data.as_ref(),
            objects,
            Some(soft_mask_resolver),
        )
    }

    /// Decodes an inline image, including its filter chain and normalized sample data.
    pub fn decode_inline_image(
        image: &InlineImage,
        objects: &dyn ObjectResolver,
        soft_mask_resolver: Option<&mut dyn SoftMaskResolver>,
    ) -> Result<Self, PdfImageError> {
        let dictionary = image.normalized_dictionary();
        let stream = StreamObject::new(0, 0, Box::new(dictionary.clone()), image.data().to_vec());
        let decoded = decode_with_resolver(&stream, objects)?;

        Self::decode_normalized_image(&dictionary, decoded.as_ref(), objects, soft_mask_resolver)
    }

    /// Decodes a normalized image dictionary and raw bytes into a raster image.
    ///
    /// The dictionary must already use canonical image keys such as `Width`,
    /// `Height`, `BitsPerComponent`, and `ColorSpace`.
    pub fn decode_normalized_image(
        dictionary: &Dictionary,
        raw_data: &[u8],
        objects: &dyn ObjectResolver,
        soft_mask_resolver: Option<&mut dyn SoftMaskResolver>,
    ) -> Result<Self, PdfImageError> {
        let metadata = Self::read_metadata(dictionary, objects)?;
        let decoded_samples = Self::decode_samples(dictionary, raw_data, objects, &metadata)?;
        Self::validate_decoded_samples(&metadata, &decoded_samples)?;
        let smask = Self::parse_optional_soft_mask(dictionary, objects, soft_mask_resolver)?;
        let (data, pixel_format) = Self::assemble_pixel_data(&metadata, &decoded_samples, smask);

        Ok(Self {
            width: metadata.width,
            height: metadata.height,
            bits_per_component: metadata.bits_per_component,
            data,
            pixel_format,
            color_space: decoded_samples.stored_color_space,
        })
    }

    /// Reads and validates the normalized image metadata from the image dictionary.
    fn read_metadata(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<ImageMetadata, PdfImageError> {
        let width = dictionary
            .get_or_err("Width")?
            .try_number::<usize>(objects)?;
        let height = dictionary
            .get_or_err("Height")?
            .try_number::<usize>(objects)?;

        if width == 0 || height == 0 {
            return Err(PdfImageError::InvalidImageDimensions { width, height });
        }

        let bits_per_component = dictionary
            .get_or_err("BitsPerComponent")?
            .try_number::<usize>(objects)?;
        let color_space = ColorSpace::from_dictionary(dictionary, objects)?;
        Self::validate_bits_per_component(bits_per_component, color_space.as_ref())?;

        let image_mask = dictionary
            .get("ImageMask")
            .map_or(Ok(false), |value| value.try_boolean(objects))?;

        Ok(ImageMetadata {
            width,
            height,
            bits_per_component,
            color_space,
            image_mask,
        })
    }

    /// Validates the allowed bit depths for indexed and non-indexed images.
    fn validate_bits_per_component(
        bits_per_component: usize,
        color_space: Option<&ColorSpace>,
    ) -> Result<(), PdfImageError> {
        if matches!(color_space, Some(ColorSpace::Indexed(_))) {
            return match bits_per_component {
                1 | 2 | 4 | 8 => Ok(()),
                _ => Err(PdfImageError::UnsupportedIndexedBits { bits_per_component }),
            };
        }

        match bits_per_component {
            1 | 8 => Ok(()),
            _ => Err(PdfImageError::UnsupportedImageBitsPerComponent { bits_per_component }),
        }
    }

    /// Decodes raw image bytes into component samples based on the configured color space.
    fn decode_samples(
        dictionary: &Dictionary,
        raw_data: &[u8],
        objects: &dyn ObjectResolver,
        metadata: &ImageMetadata,
    ) -> Result<DecodedSamples, PdfImageError> {
        match metadata.color_space.as_ref() {
            Some(ColorSpace::Indexed(indexed)) => {
                Self::decode_indexed_samples(dictionary, raw_data, objects, metadata, indexed)
            }
            _ => Self::decode_direct_samples(dictionary, raw_data, objects, metadata),
        }
    }

    /// Decodes indexed image samples, applies `/Decode`, and expands palette entries.
    fn decode_indexed_samples(
        dictionary: &Dictionary,
        raw_data: &[u8],
        objects: &dyn ObjectResolver,
        metadata: &ImageMetadata,
        indexed: &IndexedColorSpace,
    ) -> Result<DecodedSamples, PdfImageError> {
        let sample_codes = unpack_image_samples(
            raw_data,
            metadata.width,
            metadata.height,
            metadata.bits_per_component,
            1,
        )?;
        let sample_max = Self::sample_max(metadata.bits_per_component)?;
        let decode = ImageDecode::from_dictionary(
            dictionary,
            objects,
            1,
            sample_max,
            sample_max,
            metadata.image_mask,
        )?;
        let decoded_indices = decode.apply(&sample_codes);
        let base_components = indexed.base.num_color_components();
        let image_data = expand_indexed_values_to_components(
            &decoded_indices,
            &indexed.lookup,
            indexed.hival,
            base_components,
        )?;

        Ok(DecodedSamples {
            stored_color_space: Some(*indexed.base.clone()),
            num_color_components: base_components,
            image_data,
        })
    }

    /// Decodes non-indexed image samples and applies the `/Decode` transform.
    fn decode_direct_samples(
        dictionary: &Dictionary,
        raw_data: &[u8],
        objects: &dyn ObjectResolver,
        metadata: &ImageMetadata,
    ) -> Result<DecodedSamples, PdfImageError> {
        let num_components = metadata
            .color_space
            .as_ref()
            .map_or(1, ColorSpace::num_color_components);
        let sample_codes = unpack_image_samples(
            raw_data,
            metadata.width,
            metadata.height,
            metadata.bits_per_component,
            num_components,
        )?;
        let decode = ImageDecode::from_dictionary(
            dictionary,
            objects,
            num_components,
            Self::sample_max(metadata.bits_per_component)?,
            255,
            metadata.image_mask,
        )?;

        Ok(DecodedSamples {
            stored_color_space: metadata.color_space.clone(),
            num_color_components: num_components,
            image_data: decode.apply(&sample_codes),
        })
    }

    /// Ensures the decoded component stream is large enough for the declared dimensions.
    fn validate_decoded_samples(
        metadata: &ImageMetadata,
        decoded_samples: &DecodedSamples,
    ) -> Result<(), PdfImageError> {
        if decoded_samples.num_color_components == 0 {
            return Err(PdfImageError::InvalidColorComponentCount);
        }

        let num_pixels = metadata.width.saturating_mul(metadata.height);
        let expected_bytes = num_pixels.saturating_mul(decoded_samples.num_color_components);
        if decoded_samples.image_data.len() < expected_bytes {
            return Err(PdfImageError::TruncatedImageData {
                expected_bytes,
                actual_bytes: decoded_samples.image_data.len(),
            });
        }

        Ok(())
    }

    /// Resolves an optional soft mask if one is available and a resolver was provided.
    fn parse_optional_soft_mask(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        soft_mask_resolver: Option<&mut dyn SoftMaskResolver>,
    ) -> Result<Option<ImageXObject>, PdfImageError> {
        match soft_mask_resolver {
            Some(resolver) => Self::parse_smask(dictionary, objects, resolver),
            None => Ok(None),
        }
    }

    /// Resolves the `/SMask` entry and treats `/None` as an absent mask.
    fn parse_smask(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        soft_mask_resolver: &mut dyn SoftMaskResolver,
    ) -> Result<Option<ImageXObject>, PdfImageError> {
        let Some(smask_obj) = dictionary.get("SMask") else {
            return Ok(None);
        };

        let resolved = objects.resolve_object(smask_obj)?;

        if let ObjectVariant::Name(name) = resolved
            && name.as_slice() == b"None"
        {
            return Ok(None);
        }

        let stream = resolved.try_stream(objects)?;
        soft_mask_resolver.resolve_soft_mask(stream, objects)
    }

    /// Returns the maximum encoded sample value for a supported bit depth.
    fn sample_max(bits_per_component: usize) -> Result<u8, PdfImageError> {
        match bits_per_component {
            1 => Ok(1),
            2 => Ok(3),
            4 => Ok(15),
            8 => Ok(255),
            _ => Err(PdfImageError::UnsupportedImageBitsPerComponent { bits_per_component }),
        }
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
                    metadata.width,
                    metadata.height,
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
    use std::collections::BTreeMap;

    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use super::{ImageXObject, InlineImage, SoftMaskResolver};
    use crate::error::PdfImageError;

    struct TestSoftMaskResolver;

    impl SoftMaskResolver for TestSoftMaskResolver {
        fn resolve_soft_mask(
            &mut self,
            _stream: &StreamObject,
            _objects: &dyn ObjectResolver,
        ) -> Result<Option<ImageXObject>, PdfImageError> {
            Ok(None)
        }
    }

    use pdf_object::{object_resolver::ObjectResolver, stream::StreamObject};

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
        assert_eq!(image.data, vec![0x00, 0xFF, 0x00, 0xFF]);
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
        assert_eq!(image.data, vec![0x00, 0x80]);
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
        assert_eq!(image.data, vec![10, 11, 12, 255, 20, 21, 22, 255]);
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
        assert_eq!(image.data, vec![12, 34]);
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
        assert_eq!(decoded.data, vec![0x2A]);
    }

    #[test]
    fn parse_smask_name_none_is_treated_as_absent() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            "SMask".to_string(),
            ObjectVariant::Name(b"None".to_vec()),
        )]));
        let mut resolver = TestSoftMaskResolver;

        let smask = ImageXObject::parse_smask(&dictionary, &PassthroughResolver, &mut resolver)
            .expect("name-valued /SMask should be accepted");

        assert!(
            smask.is_none(),
            "/SMask /None should behave like no soft mask"
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
            image.data,
            vec![0xFF, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0x00]
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
            image.data,
            vec![
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
            image.data,
            vec![
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
            image.data,
            vec![
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
            image.data,
            vec![
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
