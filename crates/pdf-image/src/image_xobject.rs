use std::borrow::Cow;

use pdf_color_space::{color_space::ColorSpace, indexed_color_space::IndexedColorSpace};
use pdf_graphics::PixelFormat;
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};

use crate::error::PdfImageError;

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
        if bits_per_component != 1 && bits_per_component != 8 {
            return Err(PdfImageError::UnsupportedImageBitsPerComponent { bits_per_component });
        }

        let color_space = ColorSpace::from_dictionary(dictionary, objects)?;

        let raw_data = if bits_per_component == 1 {
            let n_components = color_space
                .as_ref()
                .map_or(1, ColorSpace::num_color_components);
            Cow::Owned(Self::expand_1bpc(raw_data, width, height, n_components))
        } else {
            Cow::Borrowed(raw_data)
        };

        let (image_data, stored_color_space, num_color_components): (Cow<[u8]>, _, usize) =
            match color_space {
                Some(ColorSpace::Indexed(IndexedColorSpace {
                    base,
                    hival,
                    lookup,
                })) => {
                    let base_components = base.num_color_components();
                    let expanded =
                        Self::expand_indexed(raw_data.as_ref(), base_components, hival, &lookup);
                    (Cow::Owned(expanded), Some(*base), base_components)
                }
                other => {
                    let components = match &other {
                        Some(cs) => cs.num_color_components(),
                        None => 1,
                    };
                    (raw_data, other, components)
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
                    image_data.as_ref(),
                    width,
                    height,
                    num_color_components,
                    smask.as_ref(),
                ),
                PixelFormat::RGBA8888,
            )
        } else {
            (image_data.into_owned(), PixelFormat::Gray8)
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

    fn expand_indexed(data: &[u8], base_components: usize, hival: u8, lookup: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len().saturating_mul(base_components));
        for &index in data {
            let clamped = usize::from(index.min(hival));
            let start = clamped.saturating_mul(base_components);
            let end = start.saturating_add(base_components);
            match lookup.get(start..end) {
                Some(color) => out.extend_from_slice(color),
                None => out.extend(std::iter::repeat_n(0, base_components)),
            }
        }
        out
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

    use super::{ImageXObject, SoftMaskResolver};
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
    fn decode_normalized_indexed_image_expands_palette_data() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Array(vec![
                    ObjectVariant::Name(b"Indexed".to_vec()),
                    ObjectVariant::Name(b"DeviceRGB".to_vec()),
                    ObjectVariant::Integer(1),
                    ObjectVariant::HexString(vec![10, 11, 12, 20, 21, 22]),
                ]),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(2)),
        ]));

        let image =
            ImageXObject::decode_normalized_image(&dictionary, &[0, 1], &PassthroughResolver, None)
                .expect("indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(image.data, vec![10, 11, 12, 255, 20, 21, 22, 255]);
    }
}
