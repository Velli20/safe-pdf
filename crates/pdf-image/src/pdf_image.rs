use std::collections::BTreeMap;

use bytes::Bytes;
use pdf_filter::image_payload::ImagePayload;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
    dictionary::Dictionary, object_variant::ObjectVariant, stream::StreamObject,
};

use crate::{error::PdfImageError, image_metadata::ImageMetadata};

/// An image stream's metadata and filtered payload, ready to decode into a raster.
///
/// Image XObjects are read from their stream; inline images from their dictionary
/// and data, see [`PdfImage::inline`].
#[derive(Debug)]
pub struct PdfImage {
    pub(crate) metadata: ImageMetadata,
    pub(crate) payload: ImagePayload,
}

impl FromPdfObject for PdfImage {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let object = context.object();
        Ok(Self {
            metadata: context.read(object)?,
            payload: context.read(object)?,
        })
    }
}

impl PdfImage {
    /// Reads an inline image from its parsed dictionary and encoded payload bytes.
    ///
    /// ISO 32000 restricts inline images to the filters that need no out-of-band
    /// metadata; JPXDecode is not among them. The filter chain stops at JPXDecode,
    /// so such an image is rejected without decoding its codestream.
    pub fn inline(
        dictionary: Dictionary,
        data: impl Into<Bytes>,
        access: &mut (impl ObjectAccess + ?Sized),
    ) -> Result<Self, PdfImageError> {
        let dictionary = normalize_inline_image_dictionary(&dictionary);
        let stream = StreamObject::new_encoded(0, 0, dictionary, data);
        let image = access.read::<Self>(&ObjectVariant::Stream(stream))?;
        match image.payload {
            ImagePayload::Jpx(_) => Err(PdfImageError::InlineJpxFilter),
            ImagePayload::Samples(_) => Ok(image),
        }
    }

    /// Returns whether a failed read leaves the image without samples to draw: its
    /// dimensions are missing or invalid, or its filter chain failed.
    pub fn is_unreadable(error: &ObjectReadError) -> bool {
        let ObjectReadError::Decode { source, .. } = error else {
            return false;
        };
        source.is::<pdf_filter::error::FilterError>()
            || matches!(
                source.downcast_ref::<PdfImageError>(),
                Some(PdfImageError::InvalidImageDimensions { .. })
            )
    }

    /// Returns shared ownership of the filtered payload bytes.
    pub fn shared_data(&self) -> Bytes {
        match &self.payload {
            ImagePayload::Samples(data) | ImagePayload::Jpx(data) => data.clone(),
        }
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

    use super::{PdfImage, normalize_inline_image_dictionary};

    #[test]
    fn normalize_inline_image_dictionary_expands_abbreviations() {
        let dictionary = pdf_object_reader::dictionary::Dictionary::new(BTreeMap::from([
            (Vec::from(b"BPC"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"CS"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"RGB",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"D"), ObjectVariant::Null),
            (Vec::from(b"DP"), ObjectVariant::Boolean(true)),
            (
                Vec::from(b"F"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DCTDecode",
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
                        b"DeviceRGB",
                        pdf_object_reader::string_kind::StringKind::Name
                    )
                ),
                (b"Decode".to_vec(), ObjectVariant::Null),
                (b"DecodeParms".to_vec(), ObjectVariant::Boolean(true)),
                (
                    b"Filter".to_vec(),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DCTDecode",
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
                    abbreviated.as_bytes(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            )]));

            let normalized = normalize_inline_image_dictionary(&dictionary);

            assert_eq!(
                normalized.dictionary.get(b"ColorSpace".as_slice()),
                Some(&pdf_object_reader::pdf_string::PdfString::from(
                    canonical.as_bytes(),
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
                        b"I",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"RGB",
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
                        b"Indexed",
                        pdf_object_reader::string_kind::StringKind::Name
                    ),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DeviceRGB",
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
        let image = PdfImage::inline(
            gray_dictionary(),
            data.clone(),
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        )
        .expect("unfiltered image should be constructed");

        assert_eq!(image.shared_data().as_ptr(), data.as_ptr());
    }

    #[test]
    fn inline_image_applies_filters_during_construction() {
        let mut dictionary = gray_dictionary();
        dictionary.dictionary.insert(
            b"F".to_vec(),
            pdf_object_reader::pdf_string::PdfString::from(
                b"ASCIIHexDecode",
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );

        let image = PdfImage::inline(
            dictionary,
            b"2A>".to_vec(),
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        )
        .expect("filter should decode during construction");

        assert_eq!(image.shared_data().as_ref(), &[0x2A]);
    }

    #[test]
    fn invalid_metadata_is_rejected_during_construction() {
        let error = PdfImage::inline(
            Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new()),
            vec![1],
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        )
        .expect_err("invalid metadata should prevent construction");

        assert!(matches!(
            error,
            crate::error::PdfImageError::InvalidImageDimensions { .. }
        ));
    }

    fn gray_dictionary() -> Dictionary {
        Dictionary::new(BTreeMap::from([
            (Vec::from(b"BPC"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"CS"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"G",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"H"), ObjectVariant::Integer(1)),
            (Vec::from(b"W"), ObjectVariant::Integer(2)),
        ]))
    }
}
