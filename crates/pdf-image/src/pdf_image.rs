use std::sync::Arc;

use bytes::Bytes;
use pdf_filter::{filter::Filters, image_payload::ImagePayload};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
    dictionary::Dictionary, object_variant::ObjectVariant, pdf_name::PdfName, stream::StreamObject,
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
    /// Reads an inline image from its header and encoded payload bytes.
    ///
    /// ISO 32000 restricts inline images to the filters that need no out-of-band
    /// metadata; JPXDecode is not among them. The filter chain stops at JPXDecode,
    /// so such an image is rejected without decoding its codestream.
    pub fn inline(
        header: impl Into<InlineImageHeader>,
        data: impl Into<Bytes>,
        access: &mut (impl ObjectAccess + ?Sized),
    ) -> Result<Self, PdfImageError> {
        let image = access.read::<Self>(&header.into().into_stream(data))?;
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

/// An inline image's dictionary with its abbreviated keys and colour-space names
/// expanded, read before the image data that follows it.
#[derive(Debug)]
pub struct InlineImageHeader(ObjectVariant);

impl From<Dictionary> for InlineImageHeader {
    fn from(dictionary: Dictionary) -> Self {
        Self(ObjectVariant::Dictionary(
            normalize_inline_image_dictionary(dictionary),
        ))
    }
}

impl InlineImageHeader {
    /// Returns the expanded dictionary as an object, for reading [`InlineDataLength`].
    pub fn object(&self) -> &ObjectVariant {
        &self.0
    }

    /// Combines the dictionary with the image's encoded data.
    fn into_stream(self, data: impl Into<Bytes>) -> ObjectVariant {
        let dictionary = match self.0 {
            ObjectVariant::Dictionary(dictionary) => dictionary,
            // Unreachable: a header is only built from a dictionary.
            _ => Dictionary::default(),
        };
        ObjectVariant::Stream(StreamObject::new_encoded(0, 0, dictionary, data))
    }
}

/// The byte length of an inline image's data, when its header determines it: the
/// image is unfiltered and its dimensions, depth, and component count are known.
///
/// It is a hint for finding the end of the data, so unreadable entries make it unknown
/// rather than failing the read.
pub struct InlineDataLength(pub Option<usize>);

impl FromPdfObject for InlineDataLength {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        if context.contains(Filters::KEY) {
            return Ok(Self(None));
        }
        let components = match context.lenient(b"ImageMask").unwrap_or(false) {
            true => Some(1),
            false => match context.lenient::<Arc<[u8]>>(b"ColorSpace").as_deref() {
                Some(b"DeviceGray" | b"Indexed") => Some(1),
                Some(b"DeviceRGB") => Some(3),
                Some(b"DeviceCMYK") => Some(4),
                _ => None,
            },
        };
        let length = (|| {
            let width: usize = context.lenient(b"Width")?;
            let height: usize = context.lenient(b"Height")?;
            let bits_per_component: usize = context.lenient(b"BitsPerComponent")?;
            let bits_per_row = width
                .checked_mul(components?)?
                .checked_mul(bits_per_component)?;
            height.checked_mul(bits_per_row.div_ceil(8))
        })();
        Ok(Self(length))
    }
}

/// Expands an inline-image dictionary's abbreviated keys to their image XObject names.
///
/// The parser preserves the abbreviated inline-image keys from the content stream. Where
/// both spellings of a key are present, the abbreviation wins.
fn normalize_inline_image_dictionary(dictionary: Dictionary) -> Dictionary {
    let mut normalized = Dictionary::default();
    normalized.object_number = dictionary.object_number;

    // Entries arrive in key order, where each abbreviation sorts before its full name.
    for (key, value) in dictionary {
        let canonical_key = match key.as_bytes() {
            b"W" => PdfName::from(b"Width"),
            b"H" => PdfName::from(b"Height"),
            b"BPC" => PdfName::from(b"BitsPerComponent"),
            b"CS" => PdfName::from(b"ColorSpace"),
            b"IM" => PdfName::from(b"ImageMask"),
            b"I" => PdfName::from(b"Interpolate"),
            b"F" => PdfName::from(b"Filter"),
            b"D" => PdfName::from(b"Decode"),
            b"DP" => PdfName::from(b"DecodeParms"),
            _ => key,
        };
        if normalized.get(canonical_key.as_bytes()).is_some() {
            continue;
        }

        let canonical_value = match canonical_key.as_bytes() {
            b"ColorSpace" => normalize_inline_color_space(value),
            _ => value,
        };
        normalized.insert(canonical_key, canonical_value);
    }

    normalized
}

/// Expands abbreviated device colour-space names, including inside an array.
fn normalize_inline_color_space(value: ObjectVariant) -> ObjectVariant {
    match value {
        ObjectVariant::String(name)
            if name.kind() == pdf_object_reader::string_kind::StringKind::Name =>
        {
            match name.as_bytes() {
                b"G" => ObjectVariant::name_from_bytes(b"DeviceGray"),
                b"RGB" => ObjectVariant::name_from_bytes(b"DeviceRGB"),
                b"CMYK" => ObjectVariant::name_from_bytes(b"DeviceCMYK"),
                b"I" => ObjectVariant::name_from_bytes(b"Indexed"),
                _ => ObjectVariant::String(name),
            }
        }
        ObjectVariant::Array(values) => ObjectVariant::Array(
            values
                .iter()
                .cloned()
                .map(normalize_inline_color_space)
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bytes::Bytes;
    use pdf_object_reader::{EmptySource, dictionary::Dictionary, object_variant::ObjectVariant};

    use super::{PdfImage, normalize_inline_image_dictionary};

    #[test]
    fn normalize_inline_image_dictionary_expands_abbreviations() {
        let dictionary = pdf_object_reader::dictionary::Dictionary::from_iter(BTreeMap::from([
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

        let normalized = normalize_inline_image_dictionary(dictionary);

        assert_eq!(
            normalized,
            Dictionary::from_iter(BTreeMap::from([
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
            ]))
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
            let dictionary =
                pdf_object_reader::dictionary::Dictionary::from_iter(BTreeMap::from([(
                    Vec::from(b"CS"),
                    pdf_object_reader::pdf_string::PdfString::from(
                        abbreviated.as_bytes(),
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                )]));

            let normalized = normalize_inline_image_dictionary(dictionary);

            assert_eq!(
                normalized.get(b"ColorSpace".as_slice()),
                Some(&pdf_object_reader::pdf_string::PdfString::from(
                    canonical.as_bytes(),
                    pdf_object_reader::string_kind::StringKind::Name
                ))
            );
        }
    }

    #[test]
    fn normalize_inline_image_dictionary_expands_indexed_color_space_arrays() {
        let dictionary = pdf_object_reader::dictionary::Dictionary::from_iter(BTreeMap::from([(
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

        let normalized = normalize_inline_image_dictionary(dictionary);

        assert_eq!(
            normalized.get(b"ColorSpace".as_slice()),
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
            &mut pdf_object_reader::ObjectReader::new(EmptySource).session(),
        )
        .expect("unfiltered image should be constructed");

        assert_eq!(image.shared_data().as_ptr(), data.as_ptr());
    }

    #[test]
    fn inline_image_applies_filters_during_construction() {
        let mut dictionary = gray_dictionary();
        dictionary.insert(
            b"F".to_vec(),
            pdf_object_reader::pdf_string::PdfString::from(
                b"ASCIIHexDecode",
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );

        let image = PdfImage::inline(
            dictionary,
            b"2A>".to_vec(),
            &mut pdf_object_reader::ObjectReader::new(EmptySource).session(),
        )
        .expect("filter should decode during construction");

        assert_eq!(image.shared_data().as_ref(), &[0x2A]);
    }

    #[test]
    fn invalid_metadata_is_rejected_during_construction() {
        let error = PdfImage::inline(
            Dictionary::default(),
            vec![1],
            &mut pdf_object_reader::ObjectReader::new(EmptySource).session(),
        )
        .expect_err("invalid metadata should prevent construction");

        assert!(matches!(
            error,
            crate::error::PdfImageError::InvalidImageDimensions { .. }
        ));
    }

    fn gray_dictionary() -> Dictionary {
        Dictionary::from_iter(BTreeMap::from([
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
