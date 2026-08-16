use std::collections::BTreeMap;
use std::sync::Arc;

use pdf_filter::filter::decode_data_with_resolver;
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{error::PdfImageError, image_metadata::ImageMetadata};

/// Canonical parsed representation of a PDF inline image.
#[derive(Debug)]
pub struct InlineImage {
    metadata: ImageMetadata,
    data: Arc<Vec<u8>>,
}

impl InlineImage {
    /// Creates an inline image from its parsed dictionary and encoded payload bytes.
    pub fn new(
        dictionary: Dictionary,
        data: impl Into<Arc<Vec<u8>>>,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfImageError> {
        let dictionary = normalize_inline_image_dictionary(&dictionary);
        let metadata = ImageMetadata::from_dictionary(&dictionary, objects)?;
        let data = decode_data_with_resolver(&dictionary, data.into(), objects)?;

        Ok(Self { metadata, data })
    }

    /// Returns shared ownership of the filter-decoded inline-image samples.
    pub fn shared_data(&self) -> Arc<Vec<u8>> {
        Arc::clone(&self.data)
    }

    pub(crate) fn metadata(&self) -> &ImageMetadata {
        &self.metadata
    }
}

/// Normalizes an inline-image dictionary to canonical image keys.
///
/// The parser preserves the abbreviated inline-image keys from the content stream.
/// This helper expands the keys that are commonly shared with image XObjects.
fn normalize_inline_image_dictionary(dictionary: &Dictionary) -> Dictionary {
    let mut normalized = BTreeMap::new();

    for (key, value) in &dictionary.dictionary {
        let canonical_key = match key.as_str() {
            "W" => "Width",
            "H" => "Height",
            "BPC" => "BitsPerComponent",
            "CS" => "ColorSpace",
            "IM" => "ImageMask",
            "I" => "Interpolate",
            "F" => "Filter",
            "D" => "Decode",
            "DP" => "DecodeParms",
            other => other,
        };

        let canonical_value = normalize_inline_image_value(canonical_key, value);

        normalized
            .entry(canonical_key.to_string())
            .or_insert(canonical_value);
    }

    Dictionary {
        dictionary: normalized,
        object_number: dictionary.object_number,
    }
}

fn normalize_inline_image_value(key: &str, value: &ObjectVariant) -> ObjectVariant {
    if key != "ColorSpace" {
        return value.clone();
    }

    match value {
        ObjectVariant::Name(name) => match name.as_slice() {
            b"G" => ObjectVariant::Name(b"DeviceGray".to_vec()),
            b"RGB" => ObjectVariant::Name(b"DeviceRGB".to_vec()),
            b"CMYK" => ObjectVariant::Name(b"DeviceCMYK".to_vec()),
            b"I" => ObjectVariant::Name(b"Indexed".to_vec()),
            _ => value.clone(),
        },
        ObjectVariant::Array(values) => ObjectVariant::Array(
            values
                .iter()
                .map(|item| normalize_inline_image_value("ColorSpace", item))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use super::{InlineImage, normalize_inline_image_dictionary};

    #[test]
    fn normalize_inline_image_dictionary_expands_abbreviations() {
        let dictionary = pdf_object::dictionary::Dictionary::new(BTreeMap::from([
            ("BPC".to_string(), ObjectVariant::Integer(8)),
            ("CS".to_string(), ObjectVariant::Name(b"RGB".to_vec())),
            ("D".to_string(), ObjectVariant::Null),
            ("DP".to_string(), ObjectVariant::Boolean(true)),
            ("F".to_string(), ObjectVariant::Name(b"DCTDecode".to_vec())),
            ("H".to_string(), ObjectVariant::Integer(2)),
            ("I".to_string(), ObjectVariant::Boolean(false)),
            ("IM".to_string(), ObjectVariant::Boolean(true)),
            ("W".to_string(), ObjectVariant::Integer(1)),
        ]));

        let normalized = normalize_inline_image_dictionary(&dictionary);

        assert_eq!(
            normalized.dictionary,
            BTreeMap::from([
                ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
                (
                    "ColorSpace".to_string(),
                    ObjectVariant::Name(b"DeviceRGB".to_vec())
                ),
                ("Decode".to_string(), ObjectVariant::Null),
                ("DecodeParms".to_string(), ObjectVariant::Boolean(true)),
                (
                    "Filter".to_string(),
                    ObjectVariant::Name(b"DCTDecode".to_vec())
                ),
                ("Height".to_string(), ObjectVariant::Integer(2)),
                ("ImageMask".to_string(), ObjectVariant::Boolean(true)),
                ("Interpolate".to_string(), ObjectVariant::Boolean(false)),
                ("Width".to_string(), ObjectVariant::Integer(1)),
            ])
        );
    }

    #[test]
    fn normalize_inline_image_dictionary_expands_color_space_values() {
        let cases = [
            ("G", "DeviceGray"),
            ("RGB", "DeviceRGB"),
            ("CMYK", "DeviceCMYK"),
            ("I", "Indexed"),
        ];

        for (abbreviated, canonical) in cases {
            let dictionary = pdf_object::dictionary::Dictionary::new(BTreeMap::from([(
                "CS".to_string(),
                ObjectVariant::Name(abbreviated.as_bytes().to_vec()),
            )]));

            let normalized = normalize_inline_image_dictionary(&dictionary);

            assert_eq!(
                normalized.dictionary.get("ColorSpace"),
                Some(&ObjectVariant::Name(canonical.as_bytes().to_vec()))
            );
        }
    }

    #[test]
    fn normalize_inline_image_dictionary_expands_indexed_color_space_arrays() {
        let dictionary = pdf_object::dictionary::Dictionary::new(BTreeMap::from([(
            "CS".to_string(),
            ObjectVariant::Array(vec![
                ObjectVariant::Name(b"I".to_vec()),
                ObjectVariant::Name(b"RGB".to_vec()),
                ObjectVariant::Integer(1),
                ObjectVariant::HexString(vec![10, 11, 12, 20, 21, 22]),
            ]),
        )]));

        let normalized = normalize_inline_image_dictionary(&dictionary);

        assert_eq!(
            normalized.dictionary.get("ColorSpace"),
            Some(&ObjectVariant::Array(vec![
                ObjectVariant::Name(b"Indexed".to_vec()),
                ObjectVariant::Name(b"DeviceRGB".to_vec()),
                ObjectVariant::Integer(1),
                ObjectVariant::HexString(vec![10, 11, 12, 20, 21, 22]),
            ]))
        );
    }

    #[test]
    fn inline_image_shares_unfiltered_samples() {
        let data = Arc::new(vec![1, 2]);
        let image = InlineImage::new(gray_dictionary(), Arc::clone(&data), &PassthroughResolver)
            .expect("unfiltered image should be constructed");

        assert!(Arc::ptr_eq(&image.shared_data(), &data));
    }

    #[test]
    fn inline_image_applies_filters_during_construction() {
        let mut dictionary = gray_dictionary();
        dictionary.dictionary.insert(
            "F".to_string(),
            ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
        );

        let image = InlineImage::new(dictionary, b"2A>".to_vec(), &PassthroughResolver)
            .expect("filter should decode during construction");

        assert_eq!(image.shared_data().as_ref(), &[0x2A]);
    }

    #[test]
    fn invalid_metadata_is_rejected_during_construction() {
        let error = InlineImage::new(
            Dictionary::new(BTreeMap::new()),
            vec![1],
            &PassthroughResolver,
        )
        .expect_err("invalid metadata should prevent construction");

        assert!(matches!(error, crate::error::PdfImageError::Object(_)));
    }

    fn gray_dictionary() -> Dictionary {
        Dictionary::new(BTreeMap::from([
            ("BPC".to_string(), ObjectVariant::Integer(8)),
            ("CS".to_string(), ObjectVariant::Name(b"G".to_vec())),
            ("H".to_string(), ObjectVariant::Integer(1)),
            ("W".to_string(), ObjectVariant::Integer(2)),
        ]))
    }
}
