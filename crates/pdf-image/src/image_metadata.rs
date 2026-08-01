use pdf_color_space::color_space::ColorSpace;
use pdf_filter::filter::Filters;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::error::PdfImageError;

/// Stores the normalized metadata needed to decode an image stream.
#[derive(Debug, Clone)]
pub(crate) struct ImageMetadata {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) bits_per_component: usize,
    pub(crate) color_space: Option<ColorSpace>,
    pub(crate) image_mask: bool,
    pub(crate) filters: Option<Filters>,
}

impl ImageMetadata {
    /// Reads and validates normalized image metadata from an image dictionary.
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfImageError> {
        let width = dictionary.required_number::<usize>("Width", objects)?;
        let height = dictionary.required_number::<usize>("Height", objects)?;

        if width == 0 || height == 0 {
            return Err(PdfImageError::InvalidImageDimensions { width, height });
        }

        let image_mask = dictionary
            .optional_boolean("ImageMask", objects)?
            .unwrap_or(false);
        let (bits_per_component, color_space, filters) = if image_mask {
            let bits_per_component = dictionary
                .optional_number::<usize>("BitsPerComponent", objects)?
                .unwrap_or(1);
            validate_bits_per_component(bits_per_component, image_mask, None)?;
            let filters = Filters::from_dictionary(dictionary, objects)?;
            (bits_per_component, None, filters)
        } else {
            let filters = Filters::from_dictionary(dictionary, objects)?;
            let has_jpx_filter = filters.as_ref().is_some_and(Filters::has_jpx_filter);
            let bits_per_component = if has_jpx_filter {
                dictionary
                    .optional_number::<usize>("BitsPerComponent", objects)?
                    .unwrap_or(8)
            } else {
                dictionary.required_number::<usize>("BitsPerComponent", objects)?
            };
            let color_space = ColorSpace::from_dictionary(dictionary, objects)?;
            validate_bits_per_component(bits_per_component, image_mask, color_space.as_ref())?;
            (bits_per_component, color_space, filters)
        };

        Ok(Self {
            width,
            height,
            bits_per_component,
            color_space,
            image_mask,
            filters,
        })
    }
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_filter::filter::{Filter, Filters};
    use pdf_object::{
        dictionary::Dictionary, error::ObjectError, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant,
    };

    use super::ImageMetadata;
    use crate::error::PdfImageError;

    #[test]
    fn rejects_zero_dimensions() {
        let dictionary = direct_dictionary(0, 1, 8);

        let error = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect_err("zero width should be rejected");

        assert!(matches!(
            error,
            PdfImageError::InvalidImageDimensions {
                width: 0,
                height: 1
            }
        ));
    }

    #[test]
    fn image_mask_defaults_bits_per_component_to_one() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("ImageMask".to_string(), ObjectVariant::Boolean(true)),
            ("Width".to_string(), ObjectVariant::Integer(1)),
        ]));

        let metadata = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect("image mask metadata should be valid");

        assert_eq!(metadata.bits_per_component, 1);
        assert!(metadata.color_space.is_none());
        assert!(metadata.image_mask);
        assert!(metadata.filters.is_none());
    }

    #[test]
    fn image_mask_rejects_non_one_bit_components() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("ImageMask".to_string(), ObjectVariant::Boolean(true)),
            ("Width".to_string(), ObjectVariant::Integer(1)),
        ]));

        let error = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect_err("8-bpc image mask should be rejected");

        assert!(matches!(
            error,
            PdfImageError::UnsupportedImageBitsPerComponent {
                bits_per_component: 8
            }
        ));
    }

    #[test]
    fn indexed_images_accept_two_bit_components() {
        let dictionary = indexed_dictionary(2);

        let metadata = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect("2-bpc indexed metadata should be valid");

        assert_eq!(metadata.bits_per_component, 2);
    }

    #[test]
    fn indexed_images_reject_unsupported_bit_components() {
        let dictionary = indexed_dictionary(3);

        let error = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect_err("3-bpc indexed metadata should be rejected");

        assert!(matches!(
            error,
            PdfImageError::UnsupportedIndexedBits {
                bits_per_component: 3
            }
        ));
    }

    #[test]
    fn direct_images_reject_unsupported_bit_components() {
        let dictionary = direct_dictionary(1, 1, 2);

        let error = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect_err("2-bpc direct metadata should be rejected");

        assert!(matches!(
            error,
            PdfImageError::UnsupportedImageBitsPerComponent {
                bits_per_component: 2
            }
        ));
    }

    #[test]
    fn jpx_images_default_bits_per_component_to_eight() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                "Filter".to_string(),
                ObjectVariant::Name(b"JPXDecode".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(1)),
        ]));

        let metadata = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect("JPX metadata should default missing BitsPerComponent");

        assert_eq!(metadata.bits_per_component, 8);
        assert_eq!(
            metadata.filters,
            Some(Filters::from(vec![Filter::JPXDecode]))
        );
    }

    #[test]
    fn stores_ordered_filter_chain() {
        let mut dictionary = direct_dictionary(1, 1, 8);
        dictionary.dictionary.insert(
            "Filter".to_string(),
            ObjectVariant::Array(vec![
                ObjectVariant::Name(b"ASCII85Decode".to_vec()),
                ObjectVariant::Name(b"DCTDecode".to_vec()),
            ]),
        );

        let metadata = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect("ordered filter chain should parse");

        assert_eq!(
            metadata.filters,
            Some(Filters::from(vec![
                Filter::ASCII85Decode,
                Filter::DCTDecode
            ]))
        );
    }

    #[test]
    fn non_jpx_images_require_bits_per_component() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(1)),
        ]));

        let error = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect_err("non-JPX metadata should require BitsPerComponent");

        assert!(matches!(
            error,
            PdfImageError::Object(ObjectError::MissingRequiredKey { ref key })
                if key == "BitsPerComponent"
        ));
    }

    fn direct_dictionary(width: i64, height: i64, bits_per_component: i64) -> Dictionary {
        Dictionary::new(BTreeMap::from([
            (
                "BitsPerComponent".to_string(),
                ObjectVariant::Integer(bits_per_component),
            ),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            ("Height".to_string(), ObjectVariant::Integer(height)),
            ("Width".to_string(), ObjectVariant::Integer(width)),
        ]))
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
                    ObjectVariant::Integer(1),
                    ObjectVariant::HexString(vec![0, 0, 0, 255, 255, 255]),
                ]),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(1)),
        ]))
    }
}
