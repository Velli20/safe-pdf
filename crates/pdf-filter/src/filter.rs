use std::fmt;

use bytes::Bytes;

use crate::{error::FilterError, image_payload::ImagePayload, jpeg_frame::JpegFrameHeader};

use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ObjectReader, ReadResult,
    object_resolver::PassthroughResolver, object_variant::ObjectVariant, stream::StreamObject,
};

/// Represents the compression filter applied to a stream or image in a PDF.
///
/// This corresponds to the `/Filter` entry in a PDF object's dictionary.
/// The filter specifies the algorithm used to decompress the raw stream data.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Filter {
    /// The DCT (Discrete Cosine Transform) filter, used for JPEG-compressed images.
    ///
    /// This is a lossy compression algorithm commonly used for photographic images.
    /// The decompressed data is typically in a format suitable for direct display.
    DCTDecode,
    /// The JPX (JPEG 2000) filter, used for JPEG 2000-compressed images.
    ///
    /// This is a more advanced lossy compression algorithm compared to standard JPEG,
    /// offering better compression ratios and quality at higher compression levels.
    JPXDecode,
    /// The Flate (zlib/deflate) filter, a lossless compression algorithm.
    ///
    /// Based on the zlib/deflate algorithm (RFC 1950, RFC 1951), this is one of
    /// the most commonly used filters in PDF for general-purpose compression.
    FlateDecode,
    /// The CCITT Fax filter, used for monochrome image compression.
    ///
    /// This filter is commonly used for scanned documents and fax images. It implements
    /// the CCITT Group 3 and Group 4 compression algorithms.
    CCITTFaxDecode,
    /// The ASCII base-85 filter, which decodes ASCII85-encoded stream data.
    ///
    /// ASCII85 encodes arbitrary binary data as printable ASCII characters.
    /// Every 5 ASCII characters (each in the range `!`–`u`) decode to 4 binary
    /// bytes. The special character `z` represents four zero bytes. The
    /// end-of-data marker is `~>`.
    ASCII85Decode,
    /// The ASCII hexadecimal filter, which decodes ASCIIHex-encoded stream data.
    ///
    /// ASCIIHex encodes arbitrary binary data as hexadecimal digits. Bytes
    /// that are not hexadecimal digits are ignored, `>` marks end-of-data, and
    /// a final single digit is padded with a trailing `0` nibble.
    ASCIIHexDecode,
    /// The LZW (Lempel-Ziv-Welch) filter, a lossless compression algorithm.
    ///
    /// Based on variable-length code substitution. This was used in older PDFs
    /// before FlateDecode became the standard. See PDF spec §7.4.4.
    LZWDecode,
    /// The RunLength filter, a simple byte-oriented compression algorithm.
    ///
    /// This filter stores either literal runs or repeated bytes, terminated by
    /// a dedicated end-of-data marker. See PDF spec §7.4.5.
    RunLengthDecode,
    /// The JBIG2 filter, used for monochrome bi-level images.
    ///
    /// JBIG2 decodes to tightly packed 1-bit rows, using the image stream's
    /// `Width` and `Height` entries to size the output buffer.
    JBIG2Decode,
    /// A filter that is not currently supported by this implementation.
    ///
    /// The contained string holds the original filter name from the PDF,
    /// allowing for future expansion or debugging purposes.
    Unsupported(Vec<u8>),
}

/// An ordered chain of filters applied to a PDF stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    filters: Vec<Filter>,
}

impl Filters {
    /// The dictionary key holding a stream's filter chain.
    pub const KEY: &'static [u8] = b"Filter";

    /// Returns whether the chain contains `filter`.
    pub fn has_filter(&self, filter: &Filter) -> bool {
        self.filters.contains(filter)
    }

    /// Returns whether the chain contains a JPEG 2000 decoding filter.
    pub fn has_jpx_filter(&self) -> bool {
        self.has_filter(&Filter::JPXDecode)
    }

    /// Returns whether the chain contains a JPEG decoding filter.
    pub fn has_dct_filter(&self) -> bool {
        self.has_filter(&Filter::DCTDecode)
    }
}

/// Reads a `/Filter` value: one filter name or an array applied in order.
impl FromPdfObject for Filters {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let filters = match context.object() {
            ObjectVariant::Array(_) => context.array()?.read_all()?,
            _ => vec![Filter::from_pdf_object(context)?],
        };
        Ok(Self { filters })
    }
}

/// Reads a filter name, accepting a malformed string where the name is expected.
impl FromPdfObject for Filter {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(Self::from(context.object().try_bytes(context.source())?))
    }
}

impl From<Vec<Filter>> for Filters {
    fn from(filters: Vec<Filter>) -> Self {
        Self { filters }
    }
}

impl<'a> IntoIterator for &'a Filters {
    type Item = &'a Filter;
    type IntoIter = std::slice::Iter<'a, Filter>;

    fn into_iter(self) -> Self::IntoIter {
        self.filters.iter()
    }
}

impl From<&[u8]> for Filter {
    fn from(name: &[u8]) -> Self {
        match name {
            b"DCTDecode" | b"DCT" => Self::DCTDecode,
            b"FlateDecode" | b"Fl" => Self::FlateDecode,
            b"JPXDecode" => Self::JPXDecode,
            b"CCITTFaxDecode" | b"CCF" => Self::CCITTFaxDecode,
            b"ASCII85Decode" => Self::ASCII85Decode,
            b"ASCIIHexDecode" => Self::ASCIIHexDecode,
            b"LZWDecode" | b"LZW" => Self::LZWDecode,
            b"RunLengthDecode" | b"RL" => Self::RunLengthDecode,
            b"JBIG2Decode" => Self::JBIG2Decode,
            _ => Self::Unsupported(Vec::from(name)),
        }
    }
}

impl fmt::Display for Filter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DCTDecode => f.write_str("DCTDecode"),
            Self::JPXDecode => f.write_str("JPXDecode"),
            Self::FlateDecode => f.write_str("FlateDecode"),
            Self::CCITTFaxDecode => f.write_str("CCITTFaxDecode"),
            Self::ASCII85Decode => f.write_str("ASCII85Decode"),
            Self::ASCIIHexDecode => f.write_str("ASCIIHexDecode"),
            Self::LZWDecode => f.write_str("LZWDecode"),
            Self::RunLengthDecode => f.write_str("RunLengthDecode"),
            Self::JBIG2Decode => f.write_str("JBIG2Decode"),
            Self::Unsupported(name) => write!(f, "{}", String::from_utf8_lossy(name)),
        }
    }
}

/// Individual filter decoding methods.
impl Filter {
    /// Decodes FlateDecode (zlib/deflate) compressed stream data.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Decompression`] if the zlib decompression fails,
    /// which can happen if the data is corrupted or not valid zlib-compressed data.
    pub(crate) fn decode_flate(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
        let mut decoder = flate2::read::ZlibDecoder::new(stream_data);
        let mut decoded = Vec::new();

        use std::io::Read;

        decoder
            .read_to_end(&mut decoded)
            .map_err(|e| FilterError::Decompression(e.to_string()))?;

        Ok(decoded)
    }

    /// Decodes DCTDecode (JPEG) compressed stream data.
    ///
    /// `dictionary_height` is the image dictionary's `/Height`. It replaces a
    /// frame header height of `0` or `0xFFFF`, which only stands in for the
    /// real number of lines; decoding with that placeholder would read the
    /// scans past the rows they encode.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Decompression`] if the JPEG decoding fails,
    /// which can happen if the data is corrupted or not a valid JPEG image.
    pub(crate) fn decode_jpeg_baseline(
        stream_data: &[u8],
        dictionary_height: Option<u16>,
    ) -> Result<Vec<u8>, FilterError> {
        let patched = JpegFrameHeader::find(stream_data).and_then(|header| {
            header
                .replacement_height(dictionary_height)
                .map(|height| header.with_height(stream_data, height))
        });
        let jpeg = patched.as_deref().unwrap_or(stream_data);

        let bitmap = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
            .map_err(|e| FilterError::Decompression(e.to_string()))?;

        Ok(bitmap.as_bytes().to_vec())
    }
}

/// Decodes a [`StreamObject`] by applying its full filter chain.
///
/// Unfiltered data shares the stream's existing allocation. Filtered data is
/// returned in a newly allocated shared buffer.
///
/// # Errors
///
/// Returns [`FilterError`] if a filter fails or is unsupported, except when an
/// adjacent duplicate filter fails after its first pass succeeded.
pub fn decode_with_access(
    stream: &StreamObject,
    access: &mut (impl ObjectAccess + ?Sized),
) -> Result<Bytes, FilterError> {
    ImagePayload::filtered(
        stream.shared_data(),
        &mut DictionaryContext::new(&stream.dictionary, access),
    )?
    .into_samples()
}

/// Decodes a [`StreamObject`] by applying its full filter chain.
///
/// This convenience wrapper reads without an object source, so it only
/// supports direct `/Filter` and `/DecodeParms` values.
pub fn decode(stream: &StreamObject) -> Result<Bytes, FilterError> {
    decode_with_access(
        stream,
        &mut ObjectReader::new(PassthroughResolver).session(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object_reader::{
        dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
        stream::StreamObject,
    };

    use super::*;

    #[test]
    fn test_filter_name_round_trip_ascii_hex() {
        let filter = Filter::from(b"ASCIIHexDecode".as_slice());
        assert_eq!(filter, Filter::ASCIIHexDecode);
        assert_eq!(filter.to_string(), "ASCIIHexDecode");
    }

    #[test]
    fn test_filter_name_round_trip_run_length() {
        let filter = Filter::from(b"RunLengthDecode".as_slice());
        assert_eq!(filter, Filter::RunLengthDecode);
        assert_eq!(filter.to_string(), "RunLengthDecode");
    }

    #[test]
    fn test_filter_name_round_trip_jbig2() {
        let filter = Filter::from(b"JBIG2Decode".as_slice());
        assert_eq!(filter, Filter::JBIG2Decode);
        assert_eq!(filter.to_string(), "JBIG2Decode");
    }

    #[test]
    fn filter_chain_queries_detect_corresponding_variants() {
        let filters = Filters::from(vec![Filter::ASCII85Decode, Filter::JPXDecode]);

        assert!(filters.has_filter(&Filter::ASCII85Decode));
        assert!(filters.has_jpx_filter());
        assert!(!filters.has_dct_filter());
    }

    #[test]
    fn decode_data_without_filters_preserves_shared_data() {
        let dictionary = Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new());
        let data = Bytes::from_static(b"stream data");

        let decoded = decode_with_access(
            &StreamObject::new_encoded(1, 0, dictionary, data.clone()),
            &mut ObjectReader::new(PassthroughResolver).session(),
        )
        .expect("unfiltered data should decode");

        assert_eq!(decoded.as_ptr(), data.as_ptr());
        assert_eq!(decoded.as_ref(), b"stream data");
    }

    #[test]
    fn decode_data_with_filter_returns_shared_decoded_data() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            Vec::from(b"Filter"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"ASCIIHexDecode".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        )]));

        let encoded = Bytes::from_static(b"48 65 6c 6c 6f>");
        let decoded = decode_with_access(
            &StreamObject::new_encoded(1, 0, dictionary, encoded.clone()),
            &mut ObjectReader::new(PassthroughResolver).session(),
        )
        .expect("filtered data should decode");

        assert!(decoded.is_unique());
        assert_ne!(decoded.as_ptr(), encoded.as_ptr());
        assert_eq!(decoded.as_ref(), b"Hello");
    }

    #[test]
    fn decode_unfiltered_stream_shares_data() {
        let stream = StreamObject::new(
            1,
            0,
            Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new()),
            b"shared stream data".to_vec(),
        );

        let decoded = decode(&stream).expect("unfiltered data should decode");

        assert_eq!(decoded.as_ptr(), stream.data.as_ptr());
        assert_eq!(decoded.as_ref(), b"shared stream data");
    }

    #[test]
    fn test_jbig2_missing_dimensions_returns_decode_error() {
        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"JBIG2Decode".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );

        let stream = StreamObject::new(1, 0, Dictionary::new(dict), Vec::new());
        let err = decode(&stream).expect_err("expected decode failure");
        assert!(matches!(err, FilterError::Decompression(_)));
    }

    #[test]
    fn test_jbig2_decode_parms_globals_are_resolved() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct TrackingResolver {
            objects: BTreeMap<usize, ObjectVariant>,
            resolved_globals: AtomicBool,
        }

        impl ObjectResolver for TrackingResolver {
            fn resolve_object<'a>(
                &'a self,
                obj: &'a ObjectVariant,
            ) -> Result<&'a ObjectVariant, pdf_object_reader::object_error::ObjectError>
            {
                match obj {
                    ObjectVariant::Reference(object_number) => {
                        self.objects.get(&object_number.number).ok_or(
                            pdf_object_reader::object_error::ObjectError::FailedResolveObjectReference {
                                obj_num: object_number.number,
                            },
                        )
                    }
                    other => Ok(other),
                }
            }
        }

        impl pdf_object_reader::ObjectSource for TrackingResolver {
            type Error = pdf_object_reader::object_error::ObjectError;

            fn read_object(
                &self,
                object_id: pdf_object_reader::object_id::ObjectId,
            ) -> Result<Option<pdf_object_reader::pdf_object::PdfObject>, Self::Error> {
                if object_id.number() == 3 {
                    self.resolved_globals.store(true, Ordering::Relaxed);
                }
                Ok(self
                    .objects
                    .get(&object_id.number())
                    .cloned()
                    .map(pdf_object_reader::pdf_object::PdfObject::new))
            }
        }

        let mut globals_dict = BTreeMap::new();
        globals_dict.insert(
            Vec::from(b"Filter"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"JBIG2Decode".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );
        let globals_stream = StreamObject::new(3, 0, Dictionary::new(globals_dict), Vec::new());

        let mut decode_parms = BTreeMap::new();
        decode_parms.insert(
            Vec::from(b"JBIG2Globals"),
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(3, 0)),
        );

        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"JBIG2Decode".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );
        dict.insert(Vec::from(b"Width"), ObjectVariant::Integer(8));
        dict.insert(Vec::from(b"Height"), ObjectVariant::Integer(1));
        dict.insert(
            Vec::from(b"DecodeParms"),
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(2, 0)),
        );

        let stream = StreamObject::new(1, 0, Dictionary::new(dict), Vec::new());

        let mut objects = BTreeMap::new();
        objects.insert(2, ObjectVariant::Dictionary(Dictionary::new(decode_parms)));
        objects.insert(3, ObjectVariant::Stream(globals_stream));

        let resolver = TrackingResolver {
            objects,
            resolved_globals: AtomicBool::new(false),
        };

        let _ = decode_with_access(&stream, &mut ObjectReader::new(&resolver).session())
            .expect_err("empty JBIG2 stream should fail after globals resolution");
        assert!(resolver.resolved_globals.load(Ordering::Relaxed));
    }

    #[test]
    fn test_jbig2_truncated_data_returns_decode_error() {
        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"JBIG2Decode".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );
        dict.insert(Vec::from(b"Width"), ObjectVariant::Integer(8));
        dict.insert(Vec::from(b"Height"), ObjectVariant::Integer(1));

        let stream = StreamObject::new(1, 0, Dictionary::new(dict), vec![0x00]);

        let err = decode(&stream).expect_err("expected decode failure");
        assert!(matches!(err, FilterError::Decompression(_)));
    }

    #[test]
    fn test_decode_ascii_hex_stream() {
        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"ASCIIHexDecode".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );

        let stream = StreamObject::new(
            1,
            0,
            Dictionary::new(dict),
            b"48 65 6c 6c 6f>ignored".to_vec(),
        );

        let decoded = decode(&stream).expect("decode failed");
        assert_eq!(decoded.as_ref(), b"Hello");
    }

    struct TestResolver {
        objects: BTreeMap<usize, ObjectVariant>,
    }

    impl ObjectResolver for TestResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, pdf_object_reader::object_error::ObjectError> {
            match obj {
                ObjectVariant::Reference(object_number) => self
                    .objects
                    .get(&object_number.number)
                    .ok_or(
                    pdf_object_reader::object_error::ObjectError::FailedResolveObjectReference {
                        obj_num: object_number.number,
                    },
                ),
                other => Ok(other),
            }
        }
    }

    impl pdf_object_reader::ObjectSource for TestResolver {
        type Error = pdf_object_reader::object_error::ObjectError;

        fn read_object(
            &self,
            object_id: pdf_object_reader::object_id::ObjectId,
        ) -> Result<Option<pdf_object_reader::pdf_object::PdfObject>, Self::Error> {
            Ok(self
                .objects
                .get(&object_id.number())
                .cloned()
                .map(pdf_object_reader::pdf_object::PdfObject::new))
        }
    }

    #[test]
    fn test_decode_with_indirect_filter_array() {
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"hello").expect("zlib write failed");
        let compressed = encoder.finish().expect("zlib finish failed");

        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(3, 0)),
        );
        dict.insert(
            Vec::from(b"Length"),
            ObjectVariant::Integer(compressed.len() as i64),
        );

        let stream = StreamObject::new(1, 0, Dictionary::new(dict), compressed);

        let mut objects = BTreeMap::new();
        objects.insert(
            3,
            ObjectVariant::Array(
                vec![pdf_object_reader::pdf_string::PdfString::from(
                    b"FlateDecode".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                )]
                .into(),
            ),
        );
        let resolver = TestResolver { objects };

        let decoded = decode_with_access(&stream, &mut ObjectReader::new(&resolver).session())
            .expect("decode failed");
        assert_eq!(decoded.as_ref(), b"hello");
    }

    #[test]
    fn test_decode_with_sparse_decode_parms_array() {
        use flate2::{Compression, write::ZlibEncoder};
        use std::fmt::Write as _;
        use std::io::Write as _;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"hello").expect("zlib write failed");
        let compressed = encoder.finish().expect("zlib finish failed");

        let mut encoded = String::new();
        for byte in compressed {
            write!(&mut encoded, "{byte:02x}").expect("hex write failed");
        }
        encoded.push('>');

        let dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"Filter"),
                ObjectVariant::Array(
                    vec![
                        pdf_object_reader::pdf_string::PdfString::from(
                            b"ASCIIHexDecode".to_vec(),
                            pdf_object_reader::string_kind::StringKind::Name,
                        ),
                        pdf_object_reader::pdf_string::PdfString::from(
                            b"FlateDecode".to_vec(),
                            pdf_object_reader::string_kind::StringKind::Name,
                        ),
                    ]
                    .into(),
                ),
            ),
            (
                Vec::from(b"DecodeParms"),
                ObjectVariant::Array(vec![ObjectVariant::Null].into()),
            ),
        ]));

        let decoded = decode_with_access(
            &StreamObject::new_encoded(1, 0, dictionary, encoded.into_bytes()),
            &mut ObjectReader::new(PassthroughResolver).session(),
        )
        .expect("decode failed");

        assert_eq!(decoded.as_ref(), b"hello");
    }

    #[test]
    fn test_decode_parms_array_accepts_indirect_dictionary_object() {
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write;

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"hello").expect("zlib write failed");
        let compressed = encoder.finish().expect("zlib finish failed");

        let decode_parms =
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(2, 0));
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"Filter"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"FlateDecode".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"DecodeParms"),
                ObjectVariant::Array(vec![decode_parms].into()),
            ),
        ]));

        let resolver = TestResolver {
            objects: BTreeMap::from([(
                2,
                ObjectVariant::Dictionary(Dictionary::new(
                    BTreeMap::<Vec<u8>, ObjectVariant>::new(),
                )),
            )]),
        };
        let decoded = decode_with_access(
            &StreamObject::new_encoded(1, 0, dictionary, compressed),
            &mut ObjectReader::new(&resolver).session(),
        )
        .expect("decode failed");

        assert_eq!(decoded.as_ref(), b"hello");
    }

    #[test]
    fn test_decode_run_length_stream() {
        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"RunLengthDecode".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );

        let stream = StreamObject::new(
            1,
            0,
            Dictionary::new(dict),
            vec![2, b'A', b'B', b'C', 255, b'!', 128],
        );

        let decoded = decode(&stream).expect("decode failed");
        assert_eq!(decoded.as_ref(), b"ABC!!");
    }

    #[test]
    fn test_decode_rl_alias_stream() {
        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"RL".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );

        let stream = StreamObject::new(1, 0, Dictionary::new(dict), vec![0, b'X', 128]);

        let decoded = decode(&stream).expect("decode failed");
        assert_eq!(decoded.as_ref(), b"X");
    }
}
