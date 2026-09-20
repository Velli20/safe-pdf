use pdf_color_space::color_space::ColorSpace;
use pdf_decode::DecodeMap;
use pdf_filter::filter::Filters;
use pdf_graphics::rect::Rect;
use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::error::PdfImageError;

/// Stores the normalized metadata needed to decode an image stream.
#[derive(Debug, Clone)]
pub(crate) struct ImageMetadata {
    /// Image dimensions in pixels.
    pub(crate) size: Rect<usize>,
    /// Number of bits used to encode each color component.
    pub(crate) bits_per_component: usize,
    /// Color space used to interpret component samples, or `None` for an image mask.
    pub(crate) color_space: Option<ColorSpace>,
    /// Whether the image samples form a stencil mask instead of color values.
    pub(crate) image_mask: bool,
    /// Filter chain declared by the image dictionary.
    ///
    /// The chain is retained after filtering because some decoders, such as DCT and JPX,
    /// produce samples that require filter-specific handling.
    pub(crate) filters: Option<Filters>,
    /// Optional mapping from encoded component samples to decoded component values.
    pub(crate) decode: Option<DecodeMap>,
}

impl ImageMetadata {
    /// Default bit depth for an image mask when `/BitsPerComponent` is omitted.
    const DEFAULT_IMAGE_MASK_BITS_PER_COMPONENT: usize = 1;

    /// Default bit depth for a JPX image when `/BitsPerComponent` is omitted.
    const DEFAULT_JPX_BITS_PER_COMPONENT: usize = 8;

    /// Reads and validates normalized image metadata from an image dictionary.
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfImageError> {
        let size = dictionary.required_size(objects)?;
        let width = size.width();
        let height = size.height();

        if width == 0 || height == 0 {
            return Err(PdfImageError::InvalidImageDimensions { width, height });
        }

        let image_mask = dictionary
            .optional_boolean(b"ImageMask", objects)?
            .unwrap_or(false);
        let filters = Filters::from_dictionary(dictionary, objects)?;
        let bits_per_component =
            read_bits_per_component(dictionary, objects, image_mask, filters.as_ref())?;
        let color_space = if image_mask {
            None
        } else {
            ColorSpace::from_dictionary(dictionary, objects)?
        };
        validate_bits_per_component(bits_per_component, image_mask, color_space.as_ref())?;
        let num_color_components = color_space
            .as_ref()
            .map_or(1, ColorSpace::num_color_components);
        let decode = DecodeMap::from_dictionary(dictionary, objects, num_color_components)?;

        Ok(Self {
            size,
            bits_per_component,
            color_space,
            image_mask,
            filters,
            decode,
        })
    }
}

/// Reads the image bit depth, applying the defaults allowed for masks and JPX images.
fn read_bits_per_component(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    image_mask: bool,
    filters: Option<&Filters>,
) -> Result<usize, PdfImageError> {
    const BITS_PER_COMPONENT_KEY: &[u8] = b"BitsPerComponent";

    // Image masks and JPX images may omit BitsPerComponent, with different
    // defaults. Every other image must provide the value explicitly.
    let default = if image_mask {
        Some(ImageMetadata::DEFAULT_IMAGE_MASK_BITS_PER_COMPONENT)
    } else if filters.is_some_and(Filters::has_jpx_filter) {
        Some(ImageMetadata::DEFAULT_JPX_BITS_PER_COMPONENT)
    } else {
        None
    };

    // Use the optional dictionary reader only when a default is available.
    // The required reader preserves the missing-key error for all other images.
    match default {
        Some(default) => Ok(dictionary
            .optional_number::<usize>(BITS_PER_COMPONENT_KEY, objects)?
            .unwrap_or(default)),
        None => Ok(dictionary.required_number::<usize>(BITS_PER_COMPONENT_KEY, objects)?),
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
    use pdf_object_reader::{
        dictionary::Dictionary, object_error::ObjectError, object_resolver::PassthroughResolver,
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
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"ImageMask"), ObjectVariant::Boolean(true)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
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
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"ImageMask"), ObjectVariant::Boolean(true)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
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
    fn image_mask_stores_filter_chain() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"Filter"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"ASCIIHexDecode".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"ImageMask"), ObjectVariant::Boolean(true)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let metadata = ImageMetadata::from_dictionary(&dictionary, &PassthroughResolver)
            .expect("image mask filter should parse");

        assert_eq!(
            metadata.filters,
            Some(Filters::from(vec![Filter::ASCIIHexDecode]))
        );
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
                Vec::from(b"Filter"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"JPXDecode".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
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
            b"Filter".to_vec(),
            ObjectVariant::Array(
                vec![
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"ASCII85Decode".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DCTDecode".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ]
                .into(),
            ),
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
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
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
                Vec::from(b"BitsPerComponent"),
                ObjectVariant::Integer(bits_per_component),
            ),
            (
                Vec::from(b"ColorSpace"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(height)),
            (Vec::from(b"Width"), ObjectVariant::Integer(width)),
        ]))
    }

    fn indexed_dictionary(bits_per_component: i64) -> Dictionary {
        Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"BitsPerComponent"),
                ObjectVariant::Integer(bits_per_component),
            ),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Array(
                    vec![
                        pdf_object_reader::pdf_string::PdfString::from(
                            b"Indexed".to_vec(),
                            pdf_object_reader::string_kind::StringKind::Name,
                        ),
                        pdf_object_reader::pdf_string::PdfString::from(
                            b"DeviceRGB".to_vec(),
                            pdf_object_reader::string_kind::StringKind::Name,
                        ),
                        ObjectVariant::Integer(1),
                        pdf_object_reader::pdf_string::PdfString::from(
                            vec![0, 0, 0, 255, 255, 255],
                            pdf_object_reader::string_kind::StringKind::Hexadecimal,
                        ),
                    ]
                    .into(),
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]))
    }
}
