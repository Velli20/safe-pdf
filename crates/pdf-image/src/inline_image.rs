use std::collections::BTreeMap;

use pdf_object::{dictionary::Dictionary, object_variant::ObjectVariant};

/// Canonical parsed representation of a PDF inline image.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineImage {
    dictionary: Dictionary,
    data: Vec<u8>,
}

impl InlineImage {
    /// Creates a new inline image from its parsed dictionary and raw payload bytes.
    pub fn new(dictionary: Dictionary, data: Vec<u8>) -> Self {
        Self { dictionary, data }
    }

    /// Returns the parsed inline-image dictionary.
    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// Returns the raw inline-image payload bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Splits the inline image into its parsed dictionary and raw payload.
    pub fn into_parts(self) -> (Dictionary, Vec<u8>) {
        (self.dictionary, self.data)
    }

    /// Returns a normalized copy of the inline-image dictionary.
    ///
    /// PDF inline images use abbreviated keys such as `W`, `H`, and `BPC`.
    /// Normalization maps those abbreviations to the canonical image keys so the
    /// downstream image decoder can share the same path as image XObjects.
    pub fn normalized_dictionary(&self) -> Dictionary {
        normalize_inline_image_dictionary(&self.dictionary)
    }
}

/// Normalizes an inline-image dictionary to canonical image keys.
///
/// The parser preserves the abbreviated inline-image keys from the content stream.
/// This helper expands the keys that are commonly shared with image XObjects.
pub fn normalize_inline_image_dictionary(dictionary: &Dictionary) -> Dictionary {
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
    use std::collections::BTreeMap;

    use pdf_object::object_variant::ObjectVariant;

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
    fn inline_image_normalized_dictionary_matches_helper() {
        let inline = InlineImage::new(
            pdf_object::dictionary::Dictionary::new(BTreeMap::from([(
                "W".to_string(),
                ObjectVariant::Integer(1),
            )])),
            vec![1],
        );

        assert_eq!(
            inline.normalized_dictionary().dictionary,
            normalize_inline_image_dictionary(inline.dictionary()).dictionary
        );
    }
}
