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
use crate::indexed::expand_indexed_values_to_components;

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
        let is_indexed = matches!(color_space, Some(ColorSpace::Indexed(_)));
        if is_indexed {
            if bits_per_component != 1
                && bits_per_component != 2
                && bits_per_component != 4
                && bits_per_component != 8
            {
                return Err(PdfImageError::UnsupportedIndexedBits { bits_per_component });
            }
        } else if bits_per_component != 1 && bits_per_component != 8 {
            return Err(PdfImageError::UnsupportedImageBitsPerComponent { bits_per_component });
        }

        let image_mask = dictionary
            .get("ImageMask")
            .map_or(Ok(false), |value| value.try_boolean(objects))?;

        let (stored_color_space, num_color_components, image_data) = match color_space {
            Some(ColorSpace::Indexed(IndexedColorSpace {
                base,
                hival,
                lookup,
            })) => {
                let base_components = base.num_color_components();
                let sample_codes =
                    Self::unpack_samples(raw_data, width, height, bits_per_component, 1)?;
                let decode = ImageDecode::from_dictionary(
                    dictionary,
                    objects,
                    1,
                    Self::sample_max(bits_per_component)?,
                    Self::sample_max(bits_per_component)?,
                    image_mask,
                )?;
                let decoded_indices = decode.apply(&sample_codes);
                let expanded = expand_indexed_values_to_components(
                    &decoded_indices,
                    &lookup,
                    hival,
                    base_components,
                )?;
                (Some(*base), base_components, expanded)
            }
            other => {
                let num_components = match &other {
                    Some(cs) => cs.num_color_components(),
                    None => 1,
                };
                let sample_codes = Self::unpack_samples(
                    raw_data,
                    width,
                    height,
                    bits_per_component,
                    num_components,
                )?;
                let decode = ImageDecode::from_dictionary(
                    dictionary,
                    objects,
                    num_components,
                    Self::sample_max(bits_per_component)?,
                    255,
                    image_mask,
                )?;
                let decoded_samples = decode.apply(&sample_codes);
                (other, num_components, decoded_samples)
            }
        };

        if num_color_components == 0 {
            return Err(PdfImageError::InvalidColorComponentCount);
        }

        let num_pixels = width.saturating_mul(height);
        let expected_bytes = num_pixels.saturating_mul(num_color_components);
        if image_data.len() < expected_bytes {
            return Err(PdfImageError::TruncatedImageData {
                expected_bytes,
                actual_bytes: image_data.len(),
            });
        }

        let smask = match soft_mask_resolver {
            Some(resolver) => Self::parse_smask(dictionary, objects, resolver)?,
            None => None,
        };

        let (data, pixel_format) = if smask.is_some() || num_color_components != 1 {
            (
                Self::to_rgba(
                    &image_data,
                    width,
                    height,
                    num_color_components,
                    smask.as_ref(),
                ),
                PixelFormat::RGBA8888,
            )
        } else {
            (image_data, PixelFormat::Gray8)
        };

        Ok(Self {
            width,
            height,
            bits_per_component,
            data,
            pixel_format,
            color_space: stored_color_space,
        })
    }

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

    fn sample_max(bits_per_component: usize) -> Result<u8, PdfImageError> {
        match bits_per_component {
            1 => Ok(1),
            2 => Ok(3),
            4 => Ok(15),
            8 => Ok(255),
            _ => Err(PdfImageError::UnsupportedImageBitsPerComponent { bits_per_component }),
        }
    }

    fn read_packed_sample(
        data: &[u8],
        bits_per_component: usize,
        bit_pos: &mut usize,
    ) -> Result<u8, PdfImageError> {
        let byte_index = *bit_pos / 8;
        let bit_offset = *bit_pos % 8;
        let byte = *data
            .get(byte_index)
            .ok_or_else(|| PdfImageError::TruncatedImageData {
                expected_bytes: byte_index.saturating_add(1),
                actual_bytes: data.len(),
            })?;

        let value = match bits_per_component {
            8 => u32::from(byte),
            4 => u32::from((byte >> (4usize.saturating_sub(bit_offset))) & 0x0F),
            2 => u32::from((byte >> (6usize.saturating_sub(bit_offset))) & 0x03),
            1 => u32::from((byte >> (7usize.saturating_sub(bit_offset))) & 0x01),
            _ => {
                return Err(PdfImageError::UnsupportedImageBitsPerComponent { bits_per_component });
            }
        };

        *bit_pos = bit_pos.saturating_add(bits_per_component);
        u8::try_from(value).map_err(|_| {
            PdfImageError::InvalidImageData("packed sample value cannot fit in a byte".to_string())
        })
    }

    fn unpack_samples(
        data: &[u8],
        width: usize,
        height: usize,
        bits_per_component: usize,
        num_components: usize,
    ) -> Result<Vec<u8>, PdfImageError> {
        let samples_per_row = width.saturating_mul(num_components);
        let bits_per_row = samples_per_row.saturating_mul(bits_per_component);
        let bytes_per_row = bits_per_row.saturating_add(7) / 8;
        let mut out =
            Vec::with_capacity(width.saturating_mul(height).saturating_mul(num_components));

        for row in 0..height {
            let mut bit_pos = row.saturating_mul(bytes_per_row).saturating_mul(8);
            for _ in 0..samples_per_row {
                out.push(Self::read_packed_sample(
                    data,
                    bits_per_component,
                    &mut bit_pos,
                )?);
            }
        }

        Ok(out)
    }

    #[allow(dead_code)]
    fn expand_1bpc(data: &[u8], width: usize, height: usize, num_components: usize) -> Vec<u8> {
        let bits_per_row = width.saturating_mul(num_components);
        let bytes_per_row = bits_per_row.saturating_add(7) / 8;
        let mut out =
            Vec::with_capacity(width.saturating_mul(height).saturating_mul(num_components));

        for row in 0..height {
            let row_start = row.saturating_mul(bytes_per_row);
            for col in 0..width {
                for comp in 0..num_components {
                    let bit_pos = col.saturating_mul(num_components).saturating_add(comp);
                    let byte_idx = row_start.saturating_add(bit_pos / 8);
                    let bit_idx = 7usize.saturating_sub(bit_pos % 8);
                    let bit = data.get(byte_idx).map_or(0, |b| (b >> bit_idx) & 1);
                    out.push(if bit == 1 { 0xFF } else { 0x00 });
                }
            }
        }
        out
    }

    fn to_rgba(
        image_data: &[u8],
        width: usize,
        height: usize,
        num_color_components: usize,
        smask: Option<&ImageXObject>,
    ) -> Vec<u8> {
        let num_pixels = width.saturating_mul(height);
        let smask_data = smask.map(|s| s.data.as_slice());
        let get_alpha =
            |i: usize| -> u8 { smask_data.map_or(255, |data| data.get(i).copied().unwrap_or(255)) };

        let mut out = Vec::with_capacity(num_pixels.saturating_mul(4));

        match num_color_components {
            4 => {
                for (i, chunk) in image_data.chunks_exact(4).take(num_pixels).enumerate() {
                    let &[c, m, y, k] = chunk else { continue };
                    let c_inv = 255u16.saturating_sub(u16::from(c));
                    let m_inv = 255u16.saturating_sub(u16::from(m));
                    let y_inv = 255u16.saturating_sub(u16::from(y));
                    let k_inv = 255u16.saturating_sub(u16::from(k));
                    let r = u8::try_from(c_inv.saturating_mul(k_inv) / 255).unwrap_or(0);
                    let g = u8::try_from(m_inv.saturating_mul(k_inv) / 255).unwrap_or(0);
                    let b = u8::try_from(y_inv.saturating_mul(k_inv) / 255).unwrap_or(0);
                    out.extend_from_slice(&[r, g, b, get_alpha(i)]);
                }
            }
            3 => {
                for (i, chunk) in image_data.chunks_exact(3).take(num_pixels).enumerate() {
                    let &[r, g, b] = chunk else { continue };
                    out.extend_from_slice(&[r, g, b, get_alpha(i)]);
                }
            }
            1 => {
                for (i, &gray) in image_data.iter().take(num_pixels).enumerate() {
                    out.extend_from_slice(&[gray, gray, gray, get_alpha(i)]);
                }
            }
            _ => {
                for (i, chunk) in image_data
                    .chunks_exact(num_color_components)
                    .take(num_pixels)
                    .enumerate()
                {
                    let r = chunk.first().copied().unwrap_or(0);
                    let g = chunk.get(1).copied().unwrap_or(0);
                    let b = chunk.get(2).copied().unwrap_or(0);
                    out.extend_from_slice(&[r, g, b, get_alpha(i)]);
                }
            }
        }
        out
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
    fn expand_1bpc_width_multiple_of_8() {
        let data = [0b1011_0010u8];
        let out = ImageXObject::expand_1bpc(&data, 8, 1, 1);
        assert_eq!(out.len(), 8);
        assert_eq!(out, [0xFF, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0x00]);
    }

    #[test]
    fn expand_1bpc_width_not_multiple_of_8() {
        let data = [0b1010_0000u8, 0b0110_0000u8];
        let out = ImageXObject::expand_1bpc(&data, 3, 2, 1);
        assert_eq!(out.len(), 6, "output length must be width * height");
        assert_eq!(out, [0xFF, 0x00, 0xFF, 0x00, 0xFF, 0xFF]);
    }

    #[test]
    fn expand_1bpc_all_zeros() {
        let data = [0x00u8; 4];
        let out = ImageXObject::expand_1bpc(&data, 8, 4, 1);
        assert!(out.iter().all(|&b| b == 0x00));
        assert_eq!(out.len(), 32);
    }

    #[test]
    fn expand_1bpc_all_ones() {
        let data = [0xFFu8; 4];
        let out = ImageXObject::expand_1bpc(&data, 8, 4, 1);
        assert!(out.iter().all(|&b| b == 0xFF));
        assert_eq!(out.len(), 32);
    }

    #[test]
    fn expand_1bpc_multi_component() {
        let data = [0b1101_1000u8];
        let out = ImageXObject::expand_1bpc(&data, 2, 1, 3);
        assert_eq!(out.len(), 6);
        assert_eq!(out, [0xFF, 0xFF, 0x00, 0xFF, 0xFF, 0x00]);
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
