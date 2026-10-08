use std::borrow::Cow;

use bytes::{Bytes, BytesMut};
use pdf_color_space::color_space::ColorSpace;
use pdf_decode::{
    DecodeError, DecodeMap, DecodeRange, SampleLayout, decode_sample_bytes, decode_sample_codes,
    expand_indexed_values,
};
use pdf_filter::image_payload::ImagePayload;

use crate::error::PdfImageError;
use crate::image_metadata::ImageMetadata;
use crate::jpx_image::{JpxDisplay, JpxImage};

/// Stores decoded sample bytes before the final pixel format conversion.
#[derive(Debug, Clone)]
pub(crate) struct DecodedSamples {
    pub(crate) num_color_components: usize,
    pub(crate) image_data: Bytes,
    /// Whether `image_data` is already render-ready RGBA rather than source components.
    pub(crate) is_rgba: bool,
}

impl DecodedSamples {
    /// Decodes an image stream's payload into samples for the display pipeline.
    pub(crate) fn decode(
        payload: ImagePayload,
        metadata: &ImageMetadata,
    ) -> Result<Self, PdfImageError> {
        let decoded_samples = match payload {
            ImagePayload::Jpx(codestream) => Self::decode_jpx(&codestream, metadata)?,
            ImagePayload::Samples(raw_data) => Self::decode_samples(raw_data, metadata)?,
        };

        decoded_samples.validate(metadata)?;
        Ok(decoded_samples)
    }

    /// Decodes raw image bytes into component samples based on the configured color space.
    fn decode_samples(raw_data: Bytes, metadata: &ImageMetadata) -> Result<Self, PdfImageError> {
        if let Some(decoded_samples) = Self::decode_preconverted_dct(&raw_data, metadata) {
            return Ok(decoded_samples);
        }

        match &metadata.color_space {
            Some(ColorSpace::Indexed(indexed)) => {
                if indexed.base.is_device_space() {
                    Self::decode_indexed(
                        raw_data,
                        metadata,
                        indexed.base.num_color_components(),
                        indexed.hival,
                        &indexed.lookup,
                    )
                } else {
                    Self::decode_indexed_rgba(raw_data, metadata, indexed)
                }
            }
            Some(color_space) if !color_space.is_device_space() => {
                Self::decode_direct_rgba(raw_data, metadata, color_space)
            }
            Some(color_space) => {
                Self::decode_direct(raw_data, metadata, color_space.num_color_components())
            }
            None => Self::decode_direct(raw_data, metadata, 1),
        }
    }

    /// Decodes a JPEG 2000 codestream into display samples.
    ///
    /// The codestream supplies the component count, precision, and colour
    /// description, so neither `/BitsPerComponent` nor the flattened byte
    /// length is consulted. PDF ignores `/Decode` for a JPX image, and forbids
    /// `JPXDecode` on an image mask, so no decode array is applied here.
    fn decode_jpx(codestream: &[u8], metadata: &ImageMetadata) -> Result<Self, PdfImageError> {
        let JpxDisplay {
            color_space,
            components,
            color,
            alpha,
        } = JpxImage::decode(codestream)?.into_display(
            metadata.color_space.as_ref(),
            metadata.smask_in_data,
            metadata.size.width(),
            metadata.size.height(),
        )?;

        if alpha.is_none() && color_space.is_device_space() {
            return Ok(Self {
                num_color_components: components,
                image_data: color.into(),
                is_rgba: false,
            });
        }

        // An Indexed space reads its component as a palette index rather than a
        // fraction of full scale, so the sample passes through unnormalized.
        let decoded = match color_space {
            ColorSpace::Indexed(_) => color.iter().map(|sample| f32::from(*sample)).collect(),
            ref color_space => Self::default_color_components(&color, u8::MAX, color_space),
        };
        let mut image_data = Vec::new();
        let pixels = decoded.len().checked_div(components.max(1)).unwrap_or(0);
        image_data
            .try_reserve_exact(pixels.saturating_mul(4))
            .map_err(|_| PdfImageError::JpxImageTooLarge {
                pixels,
                channels: 4,
            })?;
        for (index, pixel) in decoded.chunks_exact(components.max(1)).enumerate() {
            let [red, green, blue, _] = color_space.apply(pixel)?.to_rgba8();
            let opacity = alpha
                .as_ref()
                .and_then(|plane| plane.get(index).copied())
                .unwrap_or(u8::MAX);
            image_data.extend_from_slice(&[red, green, blue, opacity]);
        }

        Ok(Self {
            num_color_components: 4,
            image_data: image_data.into(),
            is_rgba: true,
        })
    }

    /// Ensures the decoded component stream is large enough for the declared dimensions.
    fn validate(&self, metadata: &ImageMetadata) -> Result<(), PdfImageError> {
        if self.num_color_components == 0 {
            return Err(PdfImageError::InvalidColorComponentCount);
        }

        let num_pixels = metadata.size.width().saturating_mul(metadata.size.height());
        let expected_bytes = num_pixels.saturating_mul(self.num_color_components);
        if self.image_data.len() < expected_bytes {
            return Err(PdfImageError::TruncatedImageData {
                expected_bytes,
                actual_bytes: self.image_data.len(),
            });
        }

        Ok(())
    }

    /// Uses DCT decoder output as display samples when the JPEG decoder already converted color.
    fn decode_preconverted_dct(raw_data: &Bytes, metadata: &ImageMetadata) -> Option<Self> {
        let has_dct_filter = metadata
            .filters
            .as_ref()
            .is_some_and(|filters| filters.has_dct_filter());
        if metadata.bits_per_component != 8 || !has_dct_filter {
            return None;
        }

        let num_pixels = metadata.size.width().saturating_mul(metadata.size.height());
        let num_color_components = Self::decoded_dct_component_count(raw_data.as_ref(), num_pixels)
            .or_else(|| Self::decoded_single_pixel_component_count(raw_data.as_ref()))?;

        let image_data = if raw_data.len() == num_color_components && num_pixels > 1 {
            raw_data.repeat(num_pixels).into()
        } else {
            raw_data.clone()
        };

        Some(Self {
            num_color_components,
            image_data,
            is_rgba: false,
        })
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
    fn decode_indexed(
        raw_data: Bytes,
        metadata: &ImageMetadata,
        base_color_components: usize,
        hival: u8,
        lookup: &Bytes,
    ) -> Result<Self, PdfImageError> {
        let sample_codes = Self::decode_image_sample_codes(raw_data, 1, metadata)?;
        let sample_max = Self::sample_max(metadata.sample_bits())?;
        let decoded_indices = Self::apply_decode(
            sample_codes,
            metadata.decode.as_ref(),
            sample_max,
            sample_max,
            metadata.image_mask,
        );

        let image_data = expand_indexed_values(
            decoded_indices.as_ref(),
            lookup,
            hival,
            base_color_components,
        )?;

        Ok(Self {
            num_color_components: base_color_components,
            image_data: image_data.into(),
            is_rgba: false,
        })
    }

    /// Converts an Indexed palette through a non-device base once, then maps pixels to RGBA.
    fn decode_indexed_rgba(
        raw_data: Bytes,
        metadata: &ImageMetadata,
        indexed: &pdf_color_space::indexed_color_space::IndexedColorSpace,
    ) -> Result<Self, PdfImageError> {
        let sample_codes = Self::decode_image_sample_codes(raw_data, 1, metadata)?;
        let sample_max = Self::sample_max(metadata.sample_bits())?;
        let indices = Self::apply_decode(
            sample_codes,
            metadata.decode.as_ref(),
            sample_max,
            sample_max,
            metadata.image_mask,
        );

        let indexed_color_space = ColorSpace::Indexed(indexed.clone());
        let mut palette = Vec::with_capacity(usize::from(indexed.hival).saturating_add(1));
        for index in 0..=indexed.hival {
            palette.push(indexed_color_space.apply(&[f32::from(index)])?.to_rgba8());
        }

        let mut image_data = Vec::with_capacity(indices.len().saturating_mul(4));
        for index in indices.iter().copied() {
            let bounded = index.min(indexed.hival);
            let rgba = palette.get(usize::from(bounded)).ok_or_else(|| {
                PdfImageError::InvalidImageData(format!(
                    "Indexed RGBA palette entry {bounded} is unavailable"
                ))
            })?;
            image_data.extend_from_slice(rgba);
        }

        Ok(Self {
            num_color_components: 4,
            image_data: image_data.into(),
            is_rgba: true,
        })
    }

    /// Decodes non-indexed image samples and applies the `/Decode` transform.
    fn decode_direct(
        raw_data: Bytes,
        metadata: &ImageMetadata,
        num_color_components: usize,
    ) -> Result<Self, PdfImageError> {
        let sample_codes =
            Self::decode_image_sample_codes(raw_data, num_color_components, metadata)?;
        let sample_max = Self::sample_max(metadata.sample_bits())?;

        Ok(Self {
            num_color_components,
            image_data: Self::apply_decode(
                sample_codes,
                metadata.decode.as_ref(),
                sample_max,
                255,
                metadata.image_mask,
            ),
            is_rgba: false,
        })
    }

    /// Converts decoded samples through their declared non-device color space.
    fn decode_direct_rgba(
        raw_data: Bytes,
        metadata: &ImageMetadata,
        color_space: &ColorSpace,
    ) -> Result<Self, PdfImageError> {
        let components = color_space.num_color_components();
        let sample_codes = Self::decode_image_sample_codes(raw_data, components, metadata)?;
        let sample_max = Self::sample_max(metadata.sample_bits())?;
        let decoded = metadata.decode.as_ref().map_or_else(
            || Self::default_color_components(sample_codes.as_ref(), sample_max, color_space),
            |decode| decode.apply_to_f32(sample_codes.as_ref(), sample_max),
        );
        let mut image_data = Vec::with_capacity(
            decoded
                .len()
                .checked_div(components)
                .unwrap_or(0)
                .saturating_mul(4),
        );
        for pixel in decoded.chunks_exact(components) {
            image_data.extend_from_slice(&color_space.apply(pixel)?.to_rgba8());
        }

        Ok(Self {
            num_color_components: 4,
            image_data: image_data.into(),
            is_rgba: true,
        })
    }

    /// Applies the implicit decode domains for non-device color spaces.
    fn default_color_components(
        samples: &[u8],
        sample_max: u8,
        color_space: &ColorSpace,
    ) -> Vec<f32> {
        let denominator = f32::from(sample_max.max(1));
        match color_space {
            ColorSpace::Lab(lab) => samples
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|&[l, a, b]| {
                    let [amin, amax, bmin, bmax] = lab.range;
                    let normalized = |sample| f32::from(sample) / denominator;
                    [
                        normalized(l) * 100.0,
                        amin + normalized(a) * (amax - amin),
                        bmin + normalized(b) * (bmax - bmin),
                    ]
                })
                .collect(),
            _ => samples
                .iter()
                .map(|sample| f32::from(*sample) / denominator)
                .collect(),
        }
    }

    /// Applies an explicit decode map or the implicit PDF identity/inverted default.
    fn apply_decode(
        sample_codes: Bytes,
        decode: Option<&DecodeMap>,
        sample_max: u8,
        output_max: u8,
        default_inverted: bool,
    ) -> Bytes {
        if let Some(decode) = decode {
            return decode
                .apply_to_bytes(sample_codes.as_ref(), sample_max, output_max)
                .into();
        }

        if sample_max == output_max && !default_inverted {
            return sample_codes;
        }

        let mut decoded = BytesMut::from(sample_codes);
        let default_range = if default_inverted {
            DecodeRange::inverted_identity()
        } else {
            DecodeRange::identity()
        };
        // Map every possible code once rather than every pixel.
        let mapped: Vec<u8> = (0..=sample_max)
            .map(|code| default_range.map_byte(code, sample_max, output_max))
            .collect();
        for sample in decoded.iter_mut() {
            *sample = mapped
                .get(usize::from(*sample))
                .copied()
                .unwrap_or_else(|| default_range.map_byte(*sample, sample_max, output_max));
        }
        decoded.freeze()
    }

    fn decode_image_sample_codes(
        raw_data: Bytes,
        samples_per_pixel: usize,
        metadata: &ImageMetadata,
    ) -> Result<Bytes, PdfImageError> {
        let layout = SampleLayout::RowAligned {
            width: metadata.size.width(),
            height: metadata.size.height(),
            samples_per_pixel,
        };
        if metadata.bits_per_component > 8 {
            return Self::decode_wide_sample_codes(&raw_data, metadata.bits_per_component, layout);
        }

        let sample_codes =
            decode_sample_bytes(raw_data.as_ref(), metadata.bits_per_component, layout)?;

        Ok(match sample_codes {
            Cow::Borrowed(samples) => raw_data.slice(..samples.len()),
            Cow::Owned(samples) => samples.into(),
        })
    }

    /// Decodes samples wider than a byte, keeping the most significant byte of each.
    fn decode_wide_sample_codes(
        raw_data: &[u8],
        bits_per_component: usize,
        layout: SampleLayout,
    ) -> Result<Bytes, PdfImageError> {
        let shift = u32::try_from(bits_per_component.saturating_sub(8))
            .map_err(|_| PdfImageError::UnsupportedImageBitsPerComponent { bits_per_component })?;
        decode_sample_codes(raw_data, bits_per_component, layout)?
            .into_iter()
            .map(|sample| {
                u8::try_from(sample.checked_shr(shift).unwrap_or(0))
                    .map_err(|_| PdfImageError::from(DecodeError::InvalidSampleData))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Bytes::from)
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
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object_reader::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use super::*;

    #[test]
    fn apply_decode_preserves_shared_identity_samples() {
        let samples = Bytes::from_static(&[12, 34]);
        let decoded = DecodedSamples::apply_decode(samples.clone(), None, 255, 255, false);

        assert_eq!(decoded.as_ptr(), samples.as_ptr());
    }

    #[test]
    fn decode_direct_preserves_shared_identity_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));
        let metadata = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect("direct image metadata should be valid");
        let samples = Bytes::from_static(&[12, 34]);

        let decoded = DecodedSamples::decode_direct(samples.clone(), &metadata, 1)
            .expect("identity samples should decode");

        assert_eq!(decoded.image_data.as_ptr(), samples.as_ptr());
    }
}
