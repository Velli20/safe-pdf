use pdf_color_space::color_space::ColorSpace;
use pdf_decode::DecodeMap;
use pdf_filter::filter::Filters;
use pdf_graphics::rect::Rect;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_error::ObjectError,
};

use crate::{error::PdfImageError, smask_in_data::SMaskInData};

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
    /// How a JPX image's own opacity channel is used.
    pub(crate) smask_in_data: SMaskInData,
    /// Whether the image is smoothed when it is enlarged (`/Interpolate`).
    pub(crate) interpolate: bool,
}

/// Reads an image dictionary, applying the defaults PDF allows and rejecting values
/// the decoders cannot handle; null entries count as absent.
///
/// `/ColorSpace` is decoded only for an image that is not a mask, because a mask's
/// samples carry no color and its `/ColorSpace` is never used.
impl FromPdfObject for ImageMetadata {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let size = match (
            context.lenient::<usize>(b"Width"),
            context.lenient::<usize>(b"Height"),
        ) {
            (Some(width), Some(height)) if width > 0 && height > 0 => {
                Rect::<usize>::from_size(width, height)
            }
            (width, height) => {
                return Err(PdfImageError::InvalidImageDimensions { width, height }.into());
            }
        };
        let image_mask = context.optional(b"ImageMask")?.unwrap_or_default();
        let filters = context.optional::<Filters>(Filters::KEY)?;
        let is_jpx = filters.as_ref().is_some_and(Filters::has_jpx_filter);
        let color_space = match image_mask {
            true => None,
            false => context.optional::<ColorSpace>(b"ColorSpace")?,
        };

        // Image masks and JPX images may omit /BitsPerComponent, with different
        // defaults. Every other image must provide it.
        let bits_per_component = match context.optional(b"BitsPerComponent")? {
            Some(bits_per_component) => bits_per_component,
            None if image_mask => Self::DEFAULT_IMAGE_MASK_BITS_PER_COMPONENT,
            None if is_jpx => Self::DEFAULT_JPX_BITS_PER_COMPONENT,
            None => {
                return Err(PdfImageError::Object(ObjectError::MissingRequiredKey {
                    key: "BitsPerComponent".to_string(),
                })
                .into());
            }
        };
        // PDF ignores /BitsPerComponent for a JPX image: the codestream states
        // the precision of every component, so the entry constrains nothing.
        if !is_jpx {
            validate_bits_per_component(bits_per_component, image_mask, color_space.as_ref())?;
        }
        let smask_in_data = context
            .optional::<i64>(b"SMaskInData")?
            .map_or_else(|| Ok(SMaskInData::default()), SMaskInData::try_from)?;
        let num_color_components = color_space
            .as_ref()
            .map_or(1, ColorSpace::num_color_components);
        let decode = context
            .optional::<Vec<f32>>(b"Decode")?
            .map(|values| DecodeMap::new(&values, num_color_components))
            .transpose()
            .map_err(PdfImageError::from)?;

        Ok(Self {
            size,
            bits_per_component,
            color_space,
            image_mask,
            filters,
            decode,
            smask_in_data,
            interpolate: context.optional(b"Interpolate")?.unwrap_or_default(),
        })
    }
}

impl ImageMetadata {
    /// Default bit depth for an image mask when `/BitsPerComponent` is omitted.
    const DEFAULT_IMAGE_MASK_BITS_PER_COMPONENT: usize = 1;

    /// Default bit depth for a JPX image when `/BitsPerComponent` is omitted.
    const DEFAULT_JPX_BITS_PER_COMPONENT: usize = 8;

    /// Returns the bit depth of the samples handed to the display pipeline.
    ///
    /// Sixteen-bit samples are narrowed to their most significant byte, so they
    /// are carried as eight-bit samples.
    pub(crate) fn sample_bits(&self) -> usize {
        self.bits_per_component.min(8)
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
        1 | 2 | 4 | 8 | 16 => Ok(()),
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

    fn read(dictionary: &Dictionary) -> Result<ImageMetadata, PdfImageError> {
        pdf_object_reader::ObjectReader::new(PassthroughResolver)
            .read(&ObjectVariant::Dictionary(dictionary.clone()))
            .map_err(PdfImageError::from)
    }

    #[test]
    fn rejects_zero_dimensions() {
        let dictionary = direct_dictionary(0, 1, 8);

        let error = read(&dictionary).expect_err("zero width should be rejected");

        assert!(matches!(
            error,
            PdfImageError::InvalidImageDimensions {
                width: Some(0),
                height: Some(1)
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

        let metadata = read(&dictionary).expect("image mask metadata should be valid");

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

        let error = read(&dictionary).expect_err("8-bpc image mask should be rejected");

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
                    b"ASCIIHexDecode",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"ImageMask"), ObjectVariant::Boolean(true)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let metadata = read(&dictionary).expect("image mask filter should parse");

        assert_eq!(
            metadata.filters,
            Some(Filters::from(vec![Filter::ASCIIHexDecode]))
        );
    }

    #[test]
    fn indexed_images_accept_two_bit_components() {
        let dictionary = indexed_dictionary(2);

        let metadata = read(&dictionary).expect("2-bpc indexed metadata should be valid");

        assert_eq!(metadata.bits_per_component, 2);
    }

    #[test]
    fn indexed_images_reject_unsupported_bit_components() {
        let dictionary = indexed_dictionary(3);

        let error = read(&dictionary).expect_err("3-bpc indexed metadata should be rejected");

        assert!(matches!(
            error,
            PdfImageError::UnsupportedIndexedBits {
                bits_per_component: 3
            }
        ));
    }

    #[test]
    fn direct_images_reject_unsupported_bit_components() {
        let dictionary = direct_dictionary(1, 1, 3);

        let error = read(&dictionary).expect_err("3-bpc direct metadata should be rejected");

        assert!(matches!(
            error,
            PdfImageError::UnsupportedImageBitsPerComponent {
                bits_per_component: 3
            }
        ));
    }

    #[test]
    fn jpx_images_default_bits_per_component_to_eight() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"Filter"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"JPXDecode",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let metadata =
            read(&dictionary).expect("JPX metadata should default missing BitsPerComponent");

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
                        b"ASCII85Decode",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DCTDecode",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ]
                .into(),
            ),
        );

        let metadata = read(&dictionary).expect("ordered filter chain should parse");

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

        let error =
            read(&dictionary).expect_err("non-JPX metadata should require BitsPerComponent");

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
                    b"DeviceGray",
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
                            b"Indexed",
                            pdf_object_reader::string_kind::StringKind::Name,
                        ),
                        pdf_object_reader::pdf_string::PdfString::from(
                            b"DeviceRGB",
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
