use std::fmt;

use bytes::Bytes;

use crate::{error::FilterError, predictor::PredictorParams};

use pdf_ccitt::CCITTFaxParams;
use pdf_object::{
    dictionary::Dictionary,
    object_lookup::ObjectLookupExt,
    object_resolver::{ObjectResolver, PassthroughResolver},
    object_variant::ObjectVariant,
    stream::StreamObject,
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
    /// ASCIIHex encodes arbitrary binary data as hexadecimal digits. ASCII
    /// whitespace is ignored, `>` marks end-of-data, and a final single digit
    /// is padded with a trailing `0` nibble.
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
    const KEY: &'static [u8] = b"Filter";

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

    /// Parses the `/Filter` entry from a PDF object dictionary.
    ///
    /// Returns `Ok(None)` when no `/Filter` key is present, or
    /// `Ok(Some(filters))` with the ordered chain of filters to apply.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Object`] if an object reference cannot be
    /// resolved or a name cannot be extracted.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, FilterError> {
        let Some(filter_obj) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        let resolved = objects.resolve_object(filter_obj)?;

        // Parse the `/Filter` entry: can be either a single Name or an Array of Names.
        // Per PDF spec, filters are applied in order when multiple are present.
        let filters = match resolved {
            ObjectVariant::Array(arr) => arr
                .iter()
                .map(|item| item.try_bytes(objects).map(Filter::from))
                .collect::<Result<Vec<_>, _>>()?,
            other => {
                // Accept a malformed string where the filter Name is expected.
                vec![Filter::from(other.try_bytes(objects)?)]
            }
        };

        Ok(Some(Self { filters }))
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

/// Methods for parsing the `/Filter` entry from a PDF dictionary.
impl Filter {
    /// Parses the `/Filter` entry from a PDF object dictionary.
    ///
    /// Returns `Ok(None)` when no `/Filter` key is present, or
    /// `Ok(Some(filters))` with the ordered list of filters to apply.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Object`] if an object reference cannot be
    /// resolved or a name cannot be extracted.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Vec<Filter>>, FilterError> {
        Ok(Filters::from_dictionary(dictionary, objects)?.map(|filters| filters.filters))
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
    fn decode_flate(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
        let mut decoder = flate2::read::ZlibDecoder::new(stream_data);
        let mut decoded = Vec::new();

        use std::io::Read;

        decoder
            .read_to_end(&mut decoded)
            .map_err(|e| FilterError::Decompression(e.to_string()))?;

        Ok(decoded)
    }

    /// Decodes JPXDecode (JPEG 2000) compressed stream data.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Decompression`] if the JPEG 2000 decoding fails,
    /// which can happen if the data is corrupted or not valid JPEG 2000 data.
    fn decode_jpeg2000(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
        let bitmap = jpeg2k::Image::from_bytes(stream_data)
            .map_err(|e| FilterError::Decompression(e.to_string()))?;

        let pixels = bitmap
            .get_pixels(None)
            .map_err(|e| FilterError::Decompression(e.to_string()))?;

        Self::decode_jpeg2000_pixels(pixels)
    }

    fn decode_jpeg2000_pixels(pixels: jpeg2k::ImageData) -> Result<Vec<u8>, FilterError> {
        let data = match pixels.data {
            jpeg2k::ImagePixelData::L8(data) | jpeg2k::ImagePixelData::Rgb8(data) => data,
            jpeg2k::ImagePixelData::La8(data) => data
                .chunks_exact(2)
                .map(|chunk| chunk.iter().next().copied().unwrap_or_default())
                .collect::<Vec<u8>>(),
            jpeg2k::ImagePixelData::Rgba8(data) => data
                .chunks_exact(4)
                .flat_map(|chunk| chunk.iter().take(3).copied())
                .collect::<Vec<u8>>(),
            jpeg2k::ImagePixelData::L16(data) | jpeg2k::ImagePixelData::Rgb16(data) => data
                .into_iter()
                .flat_map(|v| v.to_be_bytes())
                .collect::<Vec<u8>>(),
            jpeg2k::ImagePixelData::La16(data) => data
                .chunks_exact(2)
                .flat_map(|chunk| {
                    chunk
                        .iter()
                        .next()
                        .copied()
                        .unwrap_or_default()
                        .to_be_bytes()
                })
                .collect::<Vec<u8>>(),
            jpeg2k::ImagePixelData::Rgba16(data) => data
                .chunks_exact(4)
                .flat_map(|chunk| chunk.iter().take(3).flat_map(|v| v.to_be_bytes()))
                .collect::<Vec<u8>>(),
        };

        Ok(data)
    }

    /// Decodes DCTDecode (JPEG) compressed stream data.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Decompression`] if the JPEG decoding fails,
    /// which can happen if the data is corrupted or not a valid JPEG image.
    fn decode_jpeg_baseline(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
        let bitmap = image::load_from_memory_with_format(stream_data, image::ImageFormat::Jpeg)
            .map_err(|e| FilterError::Decompression(e.to_string()))?;

        Ok(bitmap.as_bytes().to_vec())
    }
}

/// Decodes stream data by applying the filter chain from its dictionary.
///
/// Reads the `/Filter` entry from `dictionary` and applies each filter in order.
/// Returns shared ownership of the fully decoded bytes.
/// Unfiltered input retains the original shared allocation.
///
/// This entry point accepts a dictionary and shared data separately so callers
/// such as inline-image decoders do not need to construct a temporary
/// [`StreamObject`].
///
/// # Errors
///
/// Returns [`FilterError`] if any filter in the chain fails or is unsupported.
pub fn decode_data_with_resolver(
    dictionary: &Dictionary,
    stream_data: Bytes,
    objects: &dyn ObjectResolver,
) -> Result<Bytes, FilterError> {
    let mut data = stream_data;
    let filters = Filters::from_dictionary(dictionary, objects)?;

    let Some(filters) = &filters else {
        return Ok(data);
    };

    let decode_parms = dictionary
        .get(b"DecodeParms")
        .map(|entry| objects.resolve_object(entry))
        .transpose()?;

    for (index, filter) in filters.into_iter().enumerate() {
        let param_dict = match decode_parms {
            None => None,
            Some(ObjectVariant::Dictionary(dictionary)) => Some(dictionary),
            Some(ObjectVariant::Array(array)) => {
                array.as_slice().optional_dictionary(index, objects)?
            }
            Some(other) => {
                return Err(FilterError::from(
                    pdf_object::error::ObjectError::TypeMismatch(
                        "Dictionary or Array",
                        other.name(),
                    ),
                ));
            }
        };

        match filter {
            Filter::FlateDecode => {
                let decoded = Filter::decode_flate(data.as_ref())?;
                let predictor = match param_dict {
                    Some(dictionary) => PredictorParams::from_dictionary(dictionary, objects)?,
                    None => PredictorParams::default(),
                };
                let decoded = if predictor.is_none() {
                    decoded
                } else {
                    crate::predictor::apply_predictor(&decoded, &predictor)?
                };
                data = decoded.into();
            }
            Filter::LZWDecode => {
                let (early_change, predictor) = match param_dict {
                    Some(dictionary) => (
                        dictionary
                            .optional_number(b"EarlyChange", objects)?
                            .unwrap_or(1)
                            != 0,
                        PredictorParams::from_dictionary(dictionary, objects)?,
                    ),
                    None => (true, PredictorParams::default()),
                };
                let decoded = crate::lzw::decode(data.as_ref(), early_change)?;
                let decoded = if predictor.is_none() {
                    decoded
                } else {
                    crate::predictor::apply_predictor(&decoded, &predictor)?
                };
                data = decoded.into();
            }
            Filter::JPXDecode => {
                let decoded = Filter::decode_jpeg2000(data.as_ref())?;
                data = decoded.into();
            }
            Filter::DCTDecode => {
                let decoded = Filter::decode_jpeg_baseline(data.as_ref())?;
                data = decoded.into();
            }
            Filter::ASCII85Decode => {
                let decoded = crate::ascii85::decode_ascii85(data.as_ref())?;
                data = decoded.into();
            }
            Filter::ASCIIHexDecode => {
                let decoded = crate::asciihex::decode_ascii_hex(data.as_ref())?;
                data = decoded.into();
            }
            Filter::RunLengthDecode => {
                let decoded = crate::runlength::decode_run_length(data.as_ref())?;
                data = decoded.into();
            }
            Filter::JBIG2Decode => {
                let (width, height) = resolve_jbig2_dimensions(dictionary, objects)?;
                let globals = match param_dict {
                    Some(dictionary) => resolve_jbig2_globals(dictionary, objects)?,
                    None => None,
                };
                let decoded = pdf_jbig2::decode(data.as_ref(), width, height, globals.as_deref())?;
                data = decoded.into();
            }
            Filter::CCITTFaxDecode => {
                let ccitt_params = match param_dict {
                    Some(dictionary) => CCITTFaxParams::from_dictionary(dictionary, objects)?,
                    None => CCITTFaxParams::default(),
                };
                let decoded = pdf_ccitt::decode(data.as_ref(), &ccitt_params)?;
                data = decoded.into();
            }
            Filter::Unsupported(name) => {
                return Err(FilterError::UnsupportedFilter(
                    String::from_utf8_lossy(name).into_owned(),
                ));
            }
        }
    }
    Ok(data)
}

/// Decodes a [`StreamObject`] by applying its full filter chain.
///
/// Unfiltered data shares the stream's existing allocation. Filtered data is
/// returned in a newly allocated shared buffer.
///
/// # Errors
///
/// Returns [`FilterError`] if any filter in the chain fails or is unsupported.
pub fn decode_with_resolver(
    stream: &StreamObject,
    objects: &dyn ObjectResolver,
) -> Result<Bytes, FilterError> {
    decode_data_with_resolver(&stream.dictionary, stream.shared_data(), objects)
}

/// Decodes a [`StreamObject`] by applying its full filter chain.
///
/// This convenience wrapper uses a passthrough resolver, so it only supports
/// direct `/Filter` and `/DecodeParms` values.
pub fn decode(stream: &StreamObject) -> Result<Bytes, FilterError> {
    let objects = PassthroughResolver;
    decode_with_resolver(stream, &objects)
}

fn resolve_jbig2_dimensions(
    dict: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<(u16, u16), FilterError> {
    let missing_dimensions_error =
        || FilterError::Decompression("JBIG2Decode requires positive Width and Height".into());

    let width = dict
        .optional_number::<u16>(b"Width", objects)?
        .ok_or_else(missing_dimensions_error)?;
    let height = dict
        .optional_number::<u16>(b"Height", objects)?
        .ok_or_else(missing_dimensions_error)?;
    if width == 0 || height == 0 {
        return Err(missing_dimensions_error());
    }

    Ok((width, height))
}

fn resolve_jbig2_globals(
    dict: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Option<Vec<u8>>, FilterError> {
    let Some(globals_stream) = dict.optional_stream(b"JBIG2Globals", objects)? else {
        return Ok(None);
    };

    Ok(Some(globals_stream.raw_data().to_vec()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{
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

        let decoded = decode_data_with_resolver(&dictionary, data.clone(), &PassthroughResolver)
            .expect("unfiltered data should decode");

        assert_eq!(decoded.as_ptr(), data.as_ptr());
        assert_eq!(decoded.as_ref(), b"stream data");
    }

    #[test]
    fn decode_data_with_filter_returns_shared_decoded_data() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            Vec::from(b"Filter"),
            ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
        )]));

        let encoded = Bytes::from_static(b"48 65 6c 6c 6f>");
        let decoded = decode_data_with_resolver(&dictionary, encoded.clone(), &PassthroughResolver)
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

        let decoded = decode_with_resolver(&stream, &PassthroughResolver)
            .expect("unfiltered data should decode");

        assert_eq!(decoded.as_ptr(), stream.data.as_ptr());
        assert_eq!(decoded.as_ref(), b"shared stream data");
    }

    #[test]
    fn test_jbig2_missing_dimensions_returns_decode_error() {
        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            ObjectVariant::Name(b"JBIG2Decode".to_vec()),
        );

        let stream = StreamObject::new(1, 0, Dictionary::new(dict), Vec::new());
        let err = decode(&stream).expect_err("expected decode failure");
        assert!(matches!(err, FilterError::Decompression(_)));
    }

    #[test]
    fn test_jbig2_decode_parms_globals_are_resolved() {
        use std::cell::Cell;

        struct TrackingResolver {
            objects: BTreeMap<usize, ObjectVariant>,
            resolved_globals: Cell<bool>,
        }

        impl ObjectResolver for TrackingResolver {
            fn resolve_object<'a>(
                &'a self,
                obj: &'a ObjectVariant,
            ) -> Result<&'a ObjectVariant, pdf_object::error::ObjectError> {
                match obj {
                    ObjectVariant::Reference(object_number) => {
                        if *object_number == 3 {
                            self.resolved_globals.set(true);
                        }
                        self.objects.get(object_number).ok_or(
                            pdf_object::error::ObjectError::FailedResolveObjectReference {
                                obj_num: *object_number,
                            },
                        )
                    }
                    other => Ok(other),
                }
            }
        }

        let mut globals_dict = BTreeMap::new();
        globals_dict.insert(
            Vec::from(b"Filter"),
            ObjectVariant::Name(b"JBIG2Decode".to_vec()),
        );
        let globals_stream = StreamObject::new(3, 0, Dictionary::new(globals_dict), Vec::new());

        let mut decode_parms = BTreeMap::new();
        decode_parms.insert(Vec::from(b"JBIG2Globals"), ObjectVariant::Reference(3));

        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            ObjectVariant::Name(b"JBIG2Decode".to_vec()),
        );
        dict.insert(Vec::from(b"Width"), ObjectVariant::Integer(8));
        dict.insert(Vec::from(b"Height"), ObjectVariant::Integer(1));
        dict.insert(Vec::from(b"DecodeParms"), ObjectVariant::Reference(2));

        let stream = StreamObject::new(1, 0, Dictionary::new(dict), Vec::new());

        let mut objects = BTreeMap::new();
        objects.insert(2, ObjectVariant::Dictionary(Dictionary::new(decode_parms)));
        objects.insert(3, ObjectVariant::Stream(globals_stream));

        let resolver = TrackingResolver {
            objects,
            resolved_globals: Cell::new(false),
        };

        let _ = decode_with_resolver(&stream, &resolver)
            .expect_err("empty JBIG2 stream should fail after globals resolution");
        assert!(resolver.resolved_globals.get());
    }

    #[test]
    fn test_jbig2_truncated_data_returns_decode_error() {
        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            ObjectVariant::Name(b"JBIG2Decode".to_vec()),
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
            ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
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
        ) -> Result<&'a ObjectVariant, pdf_object::error::ObjectError> {
            match obj {
                ObjectVariant::Reference(object_number) => self.objects.get(object_number).ok_or(
                    pdf_object::error::ObjectError::FailedResolveObjectReference {
                        obj_num: *object_number,
                    },
                ),
                other => Ok(other),
            }
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
        dict.insert(Vec::from(b"Filter"), ObjectVariant::Reference(3));
        dict.insert(
            Vec::from(b"Length"),
            ObjectVariant::Integer(compressed.len() as i64),
        );

        let stream = StreamObject::new(1, 0, Dictionary::new(dict), compressed);

        let mut objects = BTreeMap::new();
        objects.insert(
            3,
            ObjectVariant::Array(vec![ObjectVariant::Name(b"FlateDecode".to_vec())]),
        );
        let resolver = TestResolver { objects };

        let decoded = decode_with_resolver(&stream, &resolver).expect("decode failed");
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
                ObjectVariant::Array(vec![
                    ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
                    ObjectVariant::Name(b"FlateDecode".to_vec()),
                ]),
            ),
            (
                Vec::from(b"DecodeParms"),
                ObjectVariant::Array(vec![ObjectVariant::Null]),
            ),
        ]));

        let decoded = decode_data_with_resolver(
            &dictionary,
            encoded.into_bytes().into(),
            &PassthroughResolver,
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

        let decode_parms = ObjectVariant::Reference(2);
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"Filter"),
                ObjectVariant::Name(b"FlateDecode".to_vec()),
            ),
            (
                Vec::from(b"DecodeParms"),
                ObjectVariant::Array(vec![decode_parms]),
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
        let decoded = decode_data_with_resolver(&dictionary, compressed.into(), &resolver)
            .expect("decode failed");

        assert_eq!(decoded.as_ref(), b"hello");
    }

    #[test]
    fn test_decode_run_length_stream() {
        let mut dict = BTreeMap::new();
        dict.insert(
            Vec::from(b"Filter"),
            ObjectVariant::Name(b"RunLengthDecode".to_vec()),
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
        dict.insert(Vec::from(b"Filter"), ObjectVariant::Name(b"RL".to_vec()));

        let stream = StreamObject::new(1, 0, Dictionary::new(dict), vec![0, b'X', 128]);

        let decoded = decode(&stream).expect("decode failed");
        assert_eq!(decoded.as_ref(), b"X");
    }

    #[test]
    fn test_decode_jpeg2000_pixels_accepts_rgb8() {
        let pixels = jpeg2k::ImageData {
            width: 1,
            height: 1,
            format: jpeg2k::ImageFormat::Rgb8,
            data: jpeg2k::ImagePixelData::Rgb8(vec![10, 20, 30]),
        };

        let decoded = Filter::decode_jpeg2000_pixels(pixels).expect("decode should succeed");
        assert_eq!(decoded, vec![10, 20, 30]);
    }

    #[test]
    fn test_decode_jpeg2000_pixels_converts_la8_to_l8() {
        let pixels = jpeg2k::ImageData {
            width: 1,
            height: 2,
            format: jpeg2k::ImageFormat::La8,
            data: jpeg2k::ImagePixelData::La8(vec![11, 111, 22, 222]),
        };

        let decoded = Filter::decode_jpeg2000_pixels(pixels).expect("decode should succeed");
        assert_eq!(decoded, vec![11, 22]);
    }

    #[test]
    fn test_decode_jpeg2000_pixels_converts_rgba8_to_rgb8() {
        let pixels = jpeg2k::ImageData {
            width: 1,
            height: 2,
            format: jpeg2k::ImageFormat::Rgba8,
            data: jpeg2k::ImagePixelData::Rgba8(vec![1, 2, 3, 4, 5, 6, 7, 8]),
        };

        let decoded = Filter::decode_jpeg2000_pixels(pixels).expect("decode should succeed");
        assert_eq!(decoded, vec![1, 2, 3, 5, 6, 7]);
    }

    #[test]
    fn test_decode_jpeg2000_pixels_converts_rgba16_to_rgb16_bytes() {
        let pixels = jpeg2k::ImageData {
            width: 1,
            height: 1,
            format: jpeg2k::ImageFormat::Rgba16,
            data: jpeg2k::ImagePixelData::Rgba16(vec![0x0102, 0x0304, 0x0506, 0x0708]),
        };

        let decoded = Filter::decode_jpeg2000_pixels(pixels).expect("decode should succeed");
        assert_eq!(decoded, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
    }

    #[test]
    fn test_decode_jpeg2000_pixels_converts_la16_to_l16_bytes() {
        let pixels = jpeg2k::ImageData {
            width: 1,
            height: 2,
            format: jpeg2k::ImageFormat::La16,
            data: jpeg2k::ImagePixelData::La16(vec![0x0102, 0x0304, 0x0506, 0x0708]),
        };

        let decoded = Filter::decode_jpeg2000_pixels(pixels).expect("decode should succeed");
        assert_eq!(decoded, vec![0x01, 0x02, 0x05, 0x06]);
    }
}
