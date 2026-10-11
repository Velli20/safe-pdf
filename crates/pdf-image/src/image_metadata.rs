use pdf_color_space::color_space::ColorSpace;
use pdf_decode::DecodeMap;
use pdf_filter::filter::Filters;
use pdf_graphics::rect::Rect;
use pdf_object_reader::{
    DictionaryContext, ObjectAccess, ReadResult, object_error::ObjectError,
    object_variant::ObjectVariant,
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

impl ImageMetadata {
    /// Default bit depth for an image mask when `/BitsPerComponent` is omitted.
    const DEFAULT_IMAGE_MASK_BITS_PER_COMPONENT: usize = 1;

    /// Default bit depth for a JPX image when `/BitsPerComponent` is omitted.
    const DEFAULT_JPX_BITS_PER_COMPONENT: usize = 8;

    /// Reads and validates normalized image metadata from an image dictionary.
    pub(crate) fn read(
        context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>,
    ) -> Result<Self, PdfImageError> {
        Self::try_from(ImageEntries::read(context)?)
    }

    /// Returns the bit depth of the samples handed to the display pipeline.
    ///
    /// Sixteen-bit samples are narrowed to their most significant byte, so they
    /// are carried as eight-bit samples.
    pub(crate) fn sample_bits(&self) -> usize {
        self.bits_per_component.min(8)
    }
}

impl TryFrom<ImageEntries> for ImageMetadata {
    type Error = PdfImageError;

    /// Applies the defaults PDF allows and rejects values the decoders cannot handle.
    fn try_from(entries: ImageEntries) -> Result<Self, Self::Error> {
        let missing_key = |key: &str| ObjectError::MissingRequiredKey {
            key: key.to_string(),
        };
        let width = entries.width.ok_or_else(|| missing_key("Width"))?;
        let height = entries.height.ok_or_else(|| missing_key("Height"))?;
        if width == 0 || height == 0 {
            return Err(PdfImageError::InvalidImageDimensions { width, height });
        }

        let bits_per_component = entries
            .bits_per_component
            .or_else(|| entries.default_bits_per_component())
            .ok_or_else(|| missing_key("BitsPerComponent"))?;
        // PDF ignores /BitsPerComponent for a JPX image: the codestream states
        // the precision of every component, so the entry constrains nothing.
        if !entries.is_jpx() {
            validate_bits_per_component(
                bits_per_component,
                entries.image_mask,
                entries.color_space.as_ref(),
            )?;
        }
        let smask_in_data = entries
            .smask_in_data
            .map_or_else(|| Ok(SMaskInData::default()), SMaskInData::try_from)?;
        let num_color_components = entries
            .color_space
            .as_ref()
            .map_or(1, ColorSpace::num_color_components);
        let decode = entries
            .decode
            .map(|values| DecodeMap::new(&values, num_color_components))
            .transpose()?;

        Ok(Self {
            size: Rect::<usize>::from_size(width, height),
            bits_per_component,
            color_space: entries.color_space,
            image_mask: entries.image_mask,
            filters: entries.filters,
            decode,
            smask_in_data,
            interpolate: entries.interpolate,
        })
    }
}

/// The entries of an image dictionary, read in one pass before validation.
#[derive(Default)]
struct ImageEntries {
    width: Option<usize>,
    height: Option<usize>,
    bits_per_component: Option<usize>,
    image_mask: bool,
    color_space: Option<ColorSpace>,
    filters: Option<Filters>,
    decode: Option<Vec<f32>>,
    smask_in_data: Option<i64>,
    interpolate: bool,
}

impl ImageEntries {
    /// Reads every image entry of the context's dictionary; null entries count as absent.
    ///
    /// `/ColorSpace` is decoded only for an image that is not a mask, because a
    /// mask's samples carry no color and its `/ColorSpace` is never used.
    fn read(context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut entries = Self::default();
        let mut color_space: Option<&ObjectVariant> = None;
        for (key, value) in context.dictionary().iter() {
            if context.is_absent(value)? {
                continue;
            }
            match key {
                b"Width" => entries.width = Some(context.read(value)?),
                b"Height" => entries.height = Some(context.read(value)?),
                b"BitsPerComponent" => entries.bits_per_component = Some(context.read(value)?),
                b"ImageMask" => entries.image_mask = context.read(value)?,
                b"ColorSpace" => color_space = Some(value),
                b"Filter" => entries.filters = Some(context.read(value)?),
                b"Decode" => entries.decode = Some(context.read(value)?),
                b"SMaskInData" => entries.smask_in_data = Some(context.read(value)?),
                b"Interpolate" => entries.interpolate = context.read(value)?,
                _ => {}
            }
        }
        if !entries.image_mask {
            entries.color_space = color_space.map(|value| context.read(value)).transpose()?;
        }
        Ok(entries)
    }

    /// Returns whether the filter chain leaves a JPEG 2000 codestream.
    fn is_jpx(&self) -> bool {
        self.filters.as_ref().is_some_and(Filters::has_jpx_filter)
    }

    /// Returns the bit depth used when `/BitsPerComponent` is omitted.
    ///
    /// Image masks and JPX images may omit the entry, with different defaults.
    /// Every other image must provide it.
    fn default_bits_per_component(&self) -> Option<usize> {
        if self.image_mask {
            Some(ImageMetadata::DEFAULT_IMAGE_MASK_BITS_PER_COMPONENT)
        } else if self.is_jpx() {
            Some(ImageMetadata::DEFAULT_JPX_BITS_PER_COMPONENT)
        } else {
            None
        }
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
        DictionaryContext, dictionary::Dictionary, object_error::ObjectError,
        object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use super::ImageMetadata;
    use crate::error::PdfImageError;

    #[test]
    fn rejects_zero_dimensions() {
        let dictionary = direct_dictionary(0, 1, 8);

        let error = ImageMetadata::read(&mut DictionaryContext::new(
            &dictionary,
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        ))
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

        let metadata = ImageMetadata::read(&mut DictionaryContext::new(
            &dictionary,
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        ))
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

        let error = ImageMetadata::read(&mut DictionaryContext::new(
            &dictionary,
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        ))
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

        let metadata = ImageMetadata::read(&mut DictionaryContext::new(
            &dictionary,
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        ))
        .expect("image mask filter should parse");

        assert_eq!(
            metadata.filters,
            Some(Filters::from(vec![Filter::ASCIIHexDecode]))
        );
    }

    #[test]
    fn indexed_images_accept_two_bit_components() {
        let dictionary = indexed_dictionary(2);

        let metadata = ImageMetadata::read(&mut DictionaryContext::new(
            &dictionary,
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        ))
        .expect("2-bpc indexed metadata should be valid");

        assert_eq!(metadata.bits_per_component, 2);
    }

    #[test]
    fn indexed_images_reject_unsupported_bit_components() {
        let dictionary = indexed_dictionary(3);

        let error = ImageMetadata::read(&mut DictionaryContext::new(
            &dictionary,
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        ))
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
        let dictionary = direct_dictionary(1, 1, 3);

        let error = ImageMetadata::read(&mut DictionaryContext::new(
            &dictionary,
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        ))
        .expect_err("3-bpc direct metadata should be rejected");

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
                    b"JPXDecode".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let metadata = ImageMetadata::read(&mut DictionaryContext::new(
            &dictionary,
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        ))
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

        let metadata = ImageMetadata::read(&mut DictionaryContext::new(
            &dictionary,
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        ))
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

        let error = ImageMetadata::read(&mut DictionaryContext::new(
            &dictionary,
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        ))
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
