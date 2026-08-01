use std::{borrow::Cow, sync::Arc};

use pdf_color_space::{color_space::ColorSpace, indexed_color_space::IndexedColorSpace};
use pdf_decode::{DecodeMap, SampleLayout, decode_sample_bytes, expand_indexed_values};
use pdf_filter::filter::{Filter, decode_data_with_resolver, decode_with_resolver};
use pdf_graphics::PixelFormat;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    stream::StreamObject,
};

use crate::InlineImage;
use crate::error::PdfImageError;

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
    bits_per_component: usize,
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
        soft_mask: Option<ImageXObject>,
    ) -> Result<Self, PdfImageError> {
        match Self::decode_normalized_image(
            dictionary,
            stream_data.raw_data(),
            objects,
            soft_mask.clone(),
        ) {
            Ok(image) => Ok(image),
            Err(original_error) if Filter::from_dictionary(dictionary, objects)?.is_some() => {
                let decoded = decode_with_resolver(stream_data, objects)?;
                Self::decode_normalized_image(dictionary, decoded.as_ref(), objects, soft_mask)
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
        let metadata = Self::read_metadata(dictionary, objects)?;
        let decoded_samples = Self::decode_samples(dictionary, raw_data, objects, &metadata)?;
        Self::validate_decoded_samples(&metadata, &decoded_samples)?;
        let (data, pixel_format) =
            Self::assemble_pixel_data(&metadata, &decoded_samples, soft_mask);

        Ok(Self {
            width: metadata.width,
            height: metadata.height,
            bits_per_component: decoded_samples.bits_per_component,
            data: data.into(),
            pixel_format,
            color_space: decoded_samples.stored_color_space,
        })
    }

    /// Reads and validates the normalized image metadata from the image dictionary.
    fn read_metadata(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<ImageMetadata, PdfImageError> {
        let width = dictionary.required_number::<usize>("Width", objects)?;
        let height = dictionary.required_number::<usize>("Height", objects)?;

        if width == 0 || height == 0 {
            return Err(PdfImageError::InvalidImageDimensions { width, height });
        }

        let image_mask = dictionary
            .optional_boolean("ImageMask", objects)?
            .unwrap_or(false);
        let (bits_per_component, color_space) = if image_mask {
            let bits_per_component = dictionary
                .optional_number::<usize>("BitsPerComponent", objects)?
                .unwrap_or(1);
            Self::validate_bits_per_component(bits_per_component, image_mask, None)?;
            (bits_per_component, None)
        } else {
            let bits_per_component = if Self::has_jpx_filter(dictionary, objects)? {
                dictionary
                    .optional_number::<usize>("BitsPerComponent", objects)?
                    .unwrap_or(8)
            } else {
                dictionary.required_number::<usize>("BitsPerComponent", objects)?
            };
            let color_space = ColorSpace::from_dictionary(dictionary, objects)?;
            Self::validate_bits_per_component(
                bits_per_component,
                image_mask,
                color_space.as_ref(),
            )?;
            (bits_per_component, color_space)
        };

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
        image_mask: bool,
        color_space: Option<&ColorSpace>,
    ) -> Result<(), PdfImageError> {
        if image_mask {
            return match bits_per_component {
                1 => Ok(()),
                _ => Err(PdfImageError::UnsupportedImageBitsPerComponent { bits_per_component }),
            };
        }

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
        if let Some(decoded_samples) =
            Self::decode_preconverted_jpx_samples(dictionary, raw_data, objects, metadata)?
        {
            return Ok(decoded_samples);
        }

        if let Some(decoded_samples) =
            Self::decode_preconverted_dct_samples(dictionary, raw_data, objects, metadata)?
        {
            return Ok(decoded_samples);
        }

        match metadata.color_space.as_ref() {
            Some(ColorSpace::Indexed(indexed)) => {
                Self::decode_indexed_samples(dictionary, raw_data, objects, metadata, indexed)
            }
            _ => Self::decode_direct_samples(dictionary, raw_data, objects, metadata),
        }
    }

    /// Uses DCT decoder output as display samples when the JPEG decoder already converted color.
    fn decode_preconverted_dct_samples(
        dictionary: &Dictionary,
        raw_data: &[u8],
        objects: &dyn ObjectResolver,
        metadata: &ImageMetadata,
    ) -> Result<Option<DecodedSamples>, PdfImageError> {
        if metadata.bits_per_component != 8 || !Self::has_dct_filter(dictionary, objects)? {
            return Ok(None);
        }

        let num_pixels = metadata.width.saturating_mul(metadata.height);
        let Some(num_color_components) = Self::decoded_dct_component_count(raw_data, num_pixels)
            .or_else(|| Self::decoded_single_pixel_component_count(raw_data))
        else {
            return Ok(None);
        };

        let stored_color_space = match num_color_components {
            1 => Some(ColorSpace::DeviceGray),
            3 => Some(ColorSpace::DeviceRGB),
            _ => metadata.color_space.clone(),
        };
        let image_data = if raw_data.len() == num_color_components && num_pixels > 1 {
            raw_data.repeat(num_pixels)
        } else {
            raw_data.to_vec()
        };

        Ok(Some(DecodedSamples {
            bits_per_component: metadata.bits_per_component,
            stored_color_space,
            num_color_components,
            image_data,
        }))
    }

    /// Uses JPX decoder output as display samples when the decoder already expanded pixels.
    fn decode_preconverted_jpx_samples(
        dictionary: &Dictionary,
        raw_data: &[u8],
        objects: &dyn ObjectResolver,
        metadata: &ImageMetadata,
    ) -> Result<Option<DecodedSamples>, PdfImageError> {
        if !Self::has_jpx_filter(dictionary, objects)? {
            return Ok(None);
        }

        let num_pixels = metadata.width.saturating_mul(metadata.height);
        let Some(bytes_per_pixel) = raw_data.len().checked_div(num_pixels) else {
            return Ok(None);
        };
        if bytes_per_pixel.saturating_mul(num_pixels) != raw_data.len() {
            return Ok(None);
        }

        let (num_color_components, bits_per_component, stored_color_space) = match bytes_per_pixel {
            1 => (1, 8, Some(ColorSpace::DeviceGray)),
            2 => (1, 16, Some(ColorSpace::DeviceGray)),
            3 => (3, 8, Some(ColorSpace::DeviceRGB)),
            6 => (3, 16, Some(ColorSpace::DeviceRGB)),
            _ => return Ok(None),
        };

        Ok(Some(DecodedSamples {
            bits_per_component,
            stored_color_space,
            num_color_components,
            image_data: raw_data.to_vec(),
        }))
    }

    fn has_dct_filter(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<bool, PdfImageError> {
        Self::has_filter(dictionary, objects, Filter::DCTDecode)
    }

    fn has_jpx_filter(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<bool, PdfImageError> {
        Self::has_filter(dictionary, objects, Filter::JPXDecode)
    }

    fn has_filter(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        target: Filter,
    ) -> Result<bool, PdfImageError> {
        Ok(Filter::from_dictionary(dictionary, objects)?
            .is_some_and(|filters| filters.contains(&target)))
    }

    fn decoded_dct_component_count(raw_data: &[u8], num_pixels: usize) -> Option<usize> {
        [1, 3, 4]
            .into_iter()
            .find(|components| raw_data.len() == num_pixels.saturating_mul(*components))
    }

    fn decoded_single_pixel_component_count(raw_data: &[u8]) -> Option<usize> {
        [1, 3, 4]
            .into_iter()
            .find(|components| raw_data.len() == *components)
    }

    /// Decodes indexed image samples, applies `/Decode`, and expands palette entries.
    fn decode_indexed_samples(
        dictionary: &Dictionary,
        raw_data: &[u8],
        objects: &dyn ObjectResolver,
        metadata: &ImageMetadata,
        indexed: &IndexedColorSpace,
    ) -> Result<DecodedSamples, PdfImageError> {
        let sample_codes = Self::decode_image_sample_codes(raw_data, 1, metadata)?;
        let sample_max = Self::sample_max(metadata.bits_per_component)?;
        let decode = DecodeMap::from_dictionary(dictionary, objects, 1, metadata.image_mask)?;
        let decoded_indices = decode.apply_to_bytes(sample_codes.as_ref(), sample_max, sample_max);
        let base_components = indexed.base.num_color_components();

        let image_data = expand_indexed_values(
            &decoded_indices,
            &indexed.lookup,
            indexed.hival,
            base_components,
        )?;

        Ok(DecodedSamples {
            bits_per_component: metadata.bits_per_component,
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
        let sample_codes = Self::decode_image_sample_codes(raw_data, num_components, metadata)?;
        let decode =
            DecodeMap::from_dictionary(dictionary, objects, num_components, metadata.image_mask)?;

        Ok(DecodedSamples {
            bits_per_component: metadata.bits_per_component,
            stored_color_space: metadata.color_space.clone(),
            num_color_components: num_components,
            image_data: decode.apply_to_bytes(
                sample_codes.as_ref(),
                Self::sample_max(metadata.bits_per_component)?,
                255,
            ),
        })
    }

    fn decode_image_sample_codes<'a>(
        raw_data: &'a [u8],
        samples_per_pixel: usize,
        metadata: &ImageMetadata,
    ) -> Result<Cow<'a, [u8]>, PdfImageError> {
        Ok(decode_sample_bytes(
            raw_data,
            metadata.bits_per_component,
            SampleLayout::RowAligned {
                width: metadata.width,
                height: metadata.height,
                samples_per_pixel,
            },
        )?)
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
