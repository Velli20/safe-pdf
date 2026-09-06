use std::collections::BTreeMap;

use bytes::Bytes;
use pdf_filter::filter::decode_data_with_resolver;
use pdf_object_reader::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{error::PdfImageError, image_metadata::ImageMetadata};

/// Canonical parsed representation of a PDF inline image.
#[derive(Debug)]
pub struct InlineImage {
    metadata: ImageMetadata,
    data: Bytes,
}

impl InlineImage {
    /// Creates an inline image from its parsed dictionary and encoded payload bytes.
    pub fn new(
        dictionary: Dictionary,
        data: impl Into<Bytes>,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfImageError> {
        let dictionary = normalize_inline_image_dictionary(&dictionary);
        let metadata = ImageMetadata::from_dictionary(&dictionary, objects)?;
        let data = decode_data_with_resolver(&dictionary, data.into(), objects)?;

        Ok(Self { metadata, data })
    }

    /// Returns shared ownership of the filter-decoded inline-image samples.
    pub fn shared_data(&self) -> Bytes {
        self.data.clone()
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
        let canonical_key = match key.as_slice() {
            b"W" => b"Width".as_slice(),
            b"H" => b"Height".as_slice(),
            b"BPC" => b"BitsPerComponent".as_slice(),
            b"CS" => b"ColorSpace".as_slice(),
            b"IM" => b"ImageMask".as_slice(),
            b"I" => b"Interpolate".as_slice(),
            b"F" => b"Filter".as_slice(),
            b"D" => b"Decode".as_slice(),
            b"DP" => b"DecodeParms".as_slice(),
            other => other,
        };

        let canonical_value = normalize_inline_image_value(canonical_key, value);

        normalized
            .entry(canonical_key.to_vec())
            .or_insert(canonical_value);
    }

    Dictionary {
        dictionary: normalized,
        object_number: dictionary.object_number,
    }
}

fn normalize_inline_image_value(key: &[u8], value: &ObjectVariant) -> ObjectVariant {
    if key != b"ColorSpace" {
        return value.clone();
    }

    match value {
        ObjectVariant::String(name)
            if name.kind() == pdf_object_reader::string_kind::StringKind::Name =>
        {
            match name.as_bytes() {
                b"G" => ObjectVariant::name_from_bytes(b"DeviceGray"),
                b"RGB" => ObjectVariant::name_from_bytes(b"DeviceRGB"),
                b"CMYK" => ObjectVariant::name_from_bytes(b"DeviceCMYK"),
                b"I" => ObjectVariant::name_from_bytes(b"Indexed"),
                _ => value.clone(),
            }
        }
        ObjectVariant::Array(values) => ObjectVariant::Array(
            values
                .iter()
                .map(|item| normalize_inline_image_value(b"ColorSpace", item))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bytes::Bytes;
    use pdf_object_reader::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use super::{InlineImage, normalize_inline_image_dictionary};

    #[test]
    fn normalize_inline_image_dictionary_expands_abbreviations() {
        let dictionary = pdf_object_reader::dictionary::Dictionary::new(BTreeMap::from([
            (Vec::from(b"BPC"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"CS"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"RGB".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"D"), ObjectVariant::Null),
            (Vec::from(b"DP"), ObjectVariant::Boolean(true)),
            (
                Vec::from(b"F"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DCTDecode".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"H"), ObjectVariant::Integer(2)),
            (Vec::from(b"I"), ObjectVariant::Boolean(false)),
            (Vec::from(b"IM"), ObjectVariant::Boolean(true)),
            (Vec::from(b"W"), ObjectVariant::Integer(1)),
        ]));

        let normalized = normalize_inline_image_dictionary(&dictionary);

        assert_eq!(
            normalized.dictionary,
            BTreeMap::from([
                (b"BitsPerComponent".to_vec(), ObjectVariant::Integer(8)),
                (
                    b"ColorSpace".to_vec(),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DeviceRGB".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name
                    )
                ),
                (b"Decode".to_vec(), ObjectVariant::Null),
                (b"DecodeParms".to_vec(), ObjectVariant::Boolean(true)),
                (
                    b"Filter".to_vec(),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DCTDecode".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name
                    )
                ),
                (b"Height".to_vec(), ObjectVariant::Integer(2)),
                (b"ImageMask".to_vec(), ObjectVariant::Boolean(true)),
                (b"Interpolate".to_vec(), ObjectVariant::Boolean(false)),
                (b"Width".to_vec(), ObjectVariant::Integer(1)),
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
            let dictionary = pdf_object_reader::dictionary::Dictionary::new(BTreeMap::from([(
                Vec::from(b"CS"),
                pdf_object_reader::pdf_string::PdfString::from(
                    abbreviated.as_bytes().to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            )]));

            let normalized = normalize_inline_image_dictionary(&dictionary);

            assert_eq!(
                normalized.dictionary.get(b"ColorSpace".as_slice()),
                Some(&pdf_object_reader::pdf_string::PdfString::from(
                    canonical.as_bytes().to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name
                ))
            );
        }
    }

    #[test]
    fn normalize_inline_image_dictionary_expands_indexed_color_space_arrays() {
        let dictionary = pdf_object_reader::dictionary::Dictionary::new(BTreeMap::from([(
            Vec::from(b"CS"),
            ObjectVariant::Array(
                vec![
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"I".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"RGB".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                    ObjectVariant::Integer(1),
                    pdf_object_reader::pdf_string::PdfString::from(
                        vec![10, 11, 12, 20, 21, 22],
                        pdf_object_reader::string_kind::StringKind::Hexadecimal,
                    ),
                ]
                .into(),
            ),
        )]));

        let normalized = normalize_inline_image_dictionary(&dictionary);

        assert_eq!(
            normalized.dictionary.get(b"ColorSpace".as_slice()),
            Some(&ObjectVariant::Array(
                vec![
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"Indexed".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name.into()
                    ),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DeviceRGB".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name
                    ),
                    ObjectVariant::Integer(1),
                    pdf_object_reader::pdf_string::PdfString::from(
                        vec![10, 11, 12, 20, 21, 22],
                        pdf_object_reader::string_kind::StringKind::Hexadecimal
                    ),
                ]
                .into()
            ))
        );
    }

    #[test]
    fn inline_image_shares_unfiltered_samples() {
        let data = Bytes::from_static(&[1, 2]);
        let image = InlineImage::new(gray_dictionary(), data.clone(), &PassthroughResolver)
            .expect("unfiltered image should be constructed");

        assert_eq!(image.shared_data().as_ptr(), data.as_ptr());
    }

    #[test]
    fn inline_image_applies_filters_during_construction() {
        let mut dictionary = gray_dictionary();
        dictionary.dictionary.insert(
            b"F".to_vec(),
            pdf_object_reader::pdf_string::PdfString::from(
                b"ASCIIHexDecode".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );

        let image = InlineImage::new(dictionary, b"2A>".to_vec(), &PassthroughResolver)
            .expect("filter should decode during construction");

        assert_eq!(image.shared_data().as_ref(), &[0x2A]);
    }

    #[test]
    fn invalid_metadata_is_rejected_during_construction() {
        let error = InlineImage::new(
            Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new()),
            vec![1],
            &PassthroughResolver,
        )
        .expect_err("invalid metadata should prevent construction");

        assert!(matches!(error, crate::error::PdfImageError::Object(_)));
    }

    fn gray_dictionary() -> Dictionary {
        Dictionary::new(BTreeMap::from([
            (Vec::from(b"BPC"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"CS"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"G".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"H"), ObjectVariant::Integer(1)),
            (Vec::from(b"W"), ObjectVariant::Integer(2)),
        ]))
    }
}
