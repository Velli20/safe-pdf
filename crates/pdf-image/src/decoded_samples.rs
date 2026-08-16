use std::{borrow::Cow, sync::Arc};

use pdf_decode::{
    DecodeMap, DecodeRange, SampleLayout, decode_sample_bytes, expand_indexed_values,
};

use crate::error::PdfImageError;
use crate::image_metadata::ImageMetadata;

/// Stores decoded sample bytes before the final pixel format conversion.
#[derive(Debug, Clone)]
pub(crate) struct DecodedSamples {
    pub(crate) num_color_components: usize,
    pub(crate) image_data: Arc<Vec<u8>>,
}

impl DecodedSamples {
    /// Decodes raw image bytes into component samples based on the configured color space.
    pub(crate) fn decode(
        raw_data: Arc<Vec<u8>>,
        metadata: &ImageMetadata,
    ) -> Result<Self, PdfImageError> {
        let decoded_samples = if let Some(decoded_samples) =
            Self::decode_preconverted_jpx(&raw_data, metadata)
        {
            decoded_samples
        } else if let Some(decoded_samples) = Self::decode_preconverted_dct(&raw_data, metadata) {
            decoded_samples
        } else {
            match &metadata.color_space {
                Some(pdf_color_space::color_space::ColorSpace::Indexed(indexed)) => {
                    Self::decode_indexed(
                        raw_data,
                        metadata,
                        indexed.base.num_color_components(),
                        indexed.hival,
                        &indexed.lookup,
                    )
                }
                Some(color_space) => {
                    Self::decode_direct(raw_data, metadata, color_space.num_color_components())
                }
                None => Self::decode_direct(raw_data, metadata, 1),
            }?
        };

        decoded_samples.validate(metadata)?;
        Ok(decoded_samples)
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
    fn decode_preconverted_dct(raw_data: &Arc<Vec<u8>>, metadata: &ImageMetadata) -> Option<Self> {
        let has_dct_filter = metadata
            .filters
            .as_ref()
            .is_some_and(|filters| filters.has_dct_filter());
        if metadata.bits_per_component != 8 || !has_dct_filter {
            return None;
        }

        let num_pixels = metadata.size.width().saturating_mul(metadata.size.height());
        let num_color_components =
            Self::decoded_dct_component_count(raw_data.as_slice(), num_pixels)
                .or_else(|| Self::decoded_single_pixel_component_count(raw_data.as_slice()))?;

        let image_data = if raw_data.len() == num_color_components && num_pixels > 1 {
            Arc::new(raw_data.repeat(num_pixels))
        } else {
            Arc::clone(raw_data)
        };

        Some(Self {
            num_color_components,
            image_data,
        })
    }

    /// Uses JPX decoder output as display samples when the decoder already expanded pixels.
    fn decode_preconverted_jpx(raw_data: &Arc<Vec<u8>>, metadata: &ImageMetadata) -> Option<Self> {
        let has_jpx_filter = metadata
            .filters
            .as_ref()
            .is_some_and(|filters| filters.has_jpx_filter());
        if !has_jpx_filter {
            return None;
        }

        let num_pixels = metadata.size.width().saturating_mul(metadata.size.height());
        let bytes_per_pixel = raw_data.len().checked_div(num_pixels)?;
        if bytes_per_pixel.saturating_mul(num_pixels) != raw_data.len() {
            return None;
        }

        let num_color_components = match bytes_per_pixel {
            1 | 2 => 1,
            3 | 6 => 3,
            _ => return None,
        };

        Some(Self {
            num_color_components,
            image_data: Arc::clone(raw_data),
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
        raw_data: Arc<Vec<u8>>,
        metadata: &ImageMetadata,
        base_color_components: usize,
        hival: u8,
        lookup: &Arc<Vec<u8>>,
    ) -> Result<Self, PdfImageError> {
        let sample_codes = Self::decode_image_sample_codes(raw_data, 1, metadata)?;
        let sample_max = Self::sample_max(metadata.bits_per_component)?;
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
            image_data: Arc::new(image_data),
        })
    }

    /// Decodes non-indexed image samples and applies the `/Decode` transform.
    fn decode_direct(
        raw_data: Arc<Vec<u8>>,
        metadata: &ImageMetadata,
        num_color_components: usize,
    ) -> Result<Self, PdfImageError> {
        let sample_codes =
            Self::decode_image_sample_codes(raw_data, num_color_components, metadata)?;
        let sample_max = Self::sample_max(metadata.bits_per_component)?;

        Ok(Self {
            num_color_components,
            image_data: Self::apply_decode(
                sample_codes,
                metadata.decode.as_ref(),
                sample_max,
                255,
                metadata.image_mask,
            ),
        })
    }

    /// Applies an explicit decode map or the implicit PDF identity/inverted default.
    fn apply_decode(
        sample_codes: Arc<Vec<u8>>,
        decode: Option<&DecodeMap>,
        sample_max: u8,
        output_max: u8,
        default_inverted: bool,
    ) -> Arc<Vec<u8>> {
        if let Some(decode) = decode {
            return Arc::new(decode.apply_to_bytes(sample_codes.as_ref(), sample_max, output_max));
        }

        if sample_max == output_max && !default_inverted {
            return sample_codes;
        }

        let mut decoded = sample_codes;
        let default_range = if default_inverted {
            DecodeRange::inverted_identity()
        } else {
            DecodeRange::identity()
        };
        for sample in Arc::make_mut(&mut decoded) {
            *sample = default_range.map_byte(*sample, sample_max, output_max);
        }
        decoded
    }

    fn decode_image_sample_codes(
        raw_data: Arc<Vec<u8>>,
        samples_per_pixel: usize,
        metadata: &ImageMetadata,
    ) -> Result<Arc<Vec<u8>>, PdfImageError> {
        let sample_codes = decode_sample_bytes(
            raw_data.as_slice(),
            metadata.bits_per_component,
            SampleLayout::RowAligned {
                width: metadata.size.width(),
                height: metadata.size.height(),
                samples_per_pixel,
            },
        )?;

        Ok(match sample_codes {
            Cow::Borrowed(samples) if samples.len() == raw_data.len() => raw_data,
            Cow::Borrowed(samples) => Arc::new(samples.to_vec()),
            Cow::Owned(samples) => Arc::new(samples),
        })
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

    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use super::*;

    #[test]
    fn apply_decode_preserves_shared_identity_samples() {
        let samples = Arc::new(vec![12, 34]);
        let decoded = DecodedSamples::apply_decode(Arc::clone(&samples), None, 255, 255, false);

        assert!(Arc::ptr_eq(&decoded, &samples));
    }

    #[test]
    fn decode_direct_preserves_shared_identity_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(2)),
        ]));
        let metadata = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect("direct image metadata should be valid");
        let samples = Arc::new(vec![12, 34]);

        let decoded = DecodedSamples::decode_direct(Arc::clone(&samples), &metadata, 1)
            .expect("identity samples should decode");

        assert!(Arc::ptr_eq(&decoded.image_data, &samples));
    }
}
