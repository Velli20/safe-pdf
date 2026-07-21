use std::borrow::Cow;
use std::fmt;

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
    Unsupported(String),
}

impl From<&str> for Filter {
    fn from(name: &str) -> Self {
        match name {
            "DCTDecode" => Self::DCTDecode,
            "DCT" => Self::DCTDecode,
            "FlateDecode" => Self::FlateDecode,
            "Fl" => Self::FlateDecode,
            "JPXDecode" => Self::JPXDecode,
            "CCITTFaxDecode" => Self::CCITTFaxDecode,
            "CCF" => Self::CCITTFaxDecode,
            "ASCII85Decode" => Self::ASCII85Decode,
            "ASCIIHexDecode" => Self::ASCIIHexDecode,
            "LZWDecode" => Self::LZWDecode,
            "LZW" => Self::LZWDecode,
            "RunLengthDecode" => Self::RunLengthDecode,
            "RL" => Self::RunLengthDecode,
            "JBIG2Decode" => Self::JBIG2Decode,
            _ => Self::Unsupported(name.to_owned()),
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
            Self::Unsupported(name) => f.write_str(name),
        }
    }
}

/// Per-filter decoded parameters extracted from `/DecodeParms`.
///
/// Each variant holds the strongly-typed parameters for one filter in the
/// chain.  Variants are added as new filters gain parameter support.
#[derive(Debug, Clone)]
pub(crate) enum DecodeParms {
    /// No parameters needed or provided for this filter.
    None,
    /// Parameters for the `CCITTFaxDecode` filter.
    CcittFax(CCITTFaxParams),
    /// Parameters for `LZWDecode`: EarlyChange flag + optional predictor.
    Lzw {
        early_change: bool,
        predictor: PredictorParams,
    },
    /// Predictor parameters for `FlateDecode`.
    Flate { predictor: PredictorParams },
    /// JBIG2 globals stream data, if present.
    Jbig2 { globals: Option<Vec<u8>> },
}

/// Methods for parsing the `/Filter` entry from a PDF dictionary.
impl Filter {
    const KEY: &'static str = "Filter";

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
        let Some(filter_obj) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        let resolved = objects.resolve_object(filter_obj)?;

        // Parse the `/Filter` entry: can be either a single Name or an Array of Names.
        // Per PDF spec, filters are applied in order when multiple are present.
        let filters = match resolved {
            ObjectVariant::Array(arr) => arr
                .iter()
                .map(|item| item.try_str(objects).map(Filter::from))
                .collect::<Result<Vec<_>, _>>()?,
            other => {
                // Handle single name that wasn't parsed as Name variant
                vec![Filter::from(other.try_str(objects)?)]
            }
        };

        Ok(Some(filters))
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

/// Decodes borrowed stream data by applying the filter chain from its dictionary.
///
/// Reads the `/Filter` entry from `dictionary` and applies each filter in order.
/// Returns the fully decoded bytes, or `Cow::Borrowed` if no filters are present.
///
/// This entry point accepts a dictionary and data slice separately so callers
/// such as inline-image decoders do not need to construct a temporary
/// [`StreamObject`].
///
/// # Errors
///
/// Returns [`FilterError`] if any filter in the chain fails or is unsupported.
pub fn decode_data_with_resolver<'a>(
    dictionary: &Dictionary,
    stream_data: &'a [u8],
    objects: &dyn ObjectResolver,
) -> Result<Cow<'a, [u8]>, FilterError> {
    let mut data: Cow<'a, [u8]> = Cow::Borrowed(stream_data);
    let filters = Filter::from_dictionary(dictionary, objects)?;

    let Some(filters) = &filters else {
        return Ok(data);
    };

    let decode_params = parse_decode_params(dictionary, filters, objects)?;

    for (filter, params) in filters.iter().zip(decode_params.iter()) {
        match filter {
            Filter::FlateDecode => {
                let decoded = Filter::decode_flate(&data)?;
                let decoded = match params {
                    DecodeParms::Flate { predictor } if !predictor.is_none() => {
                        crate::predictor::apply_predictor(&decoded, predictor)?
                    }
                    _ => decoded,
                };
                data = Cow::Owned(decoded);
            }
            Filter::LZWDecode => {
                let (early_change, predictor) = match params {
                    DecodeParms::Lzw {
                        early_change,
                        predictor,
                    } => (*early_change, Some(predictor)),
                    _ => (true, None),
                };
                let decoded = crate::lzw::decode(&data, early_change)?;
                let decoded = match predictor {
                    Some(p) if !p.is_none() => crate::predictor::apply_predictor(&decoded, p)?,
                    _ => decoded,
                };
                data = Cow::Owned(decoded);
            }
            Filter::JPXDecode => {
                let decoded = Filter::decode_jpeg2000(&data)?;
                data = Cow::Owned(decoded);
            }
            Filter::DCTDecode => {
                let decoded = Filter::decode_jpeg_baseline(&data)?;
                data = Cow::Owned(decoded);
            }
            Filter::ASCII85Decode => {
                let decoded = crate::ascii85::decode_ascii85(&data)?;
                data = Cow::Owned(decoded);
            }
            Filter::ASCIIHexDecode => {
                let decoded = crate::asciihex::decode_ascii_hex(&data)?;
                data = Cow::Owned(decoded);
            }
            Filter::RunLengthDecode => {
                let decoded = crate::runlength::decode_run_length(&data)?;
                data = Cow::Owned(decoded);
            }
            Filter::JBIG2Decode => {
                let (width, height) = resolve_jbig2_dimensions(dictionary, objects)?;
                let globals = match params {
                    DecodeParms::Jbig2 { globals } => globals.as_deref(),
                    _ => None,
                };
                let decoded = pdf_jbig2::decode(&data, width, height, globals)?;
                data = Cow::Owned(decoded);
            }
            Filter::CCITTFaxDecode => {
                let ccitt_params = match params {
                    DecodeParms::CcittFax(p) => p,
                    _ => &CCITTFaxParams::DEFAULT,
                };
                let decoded = pdf_ccitt::decode(&data, ccitt_params)?;
                data = Cow::Owned(decoded);
            }
            Filter::Unsupported(name) => {
                return Err(FilterError::UnsupportedFilter(name.clone()));
            }
        }
    }
    Ok(data)
}

/// Decodes a [`StreamObject`] by applying its full filter chain.
///
/// This compatibility entry point forwards the stream's dictionary and raw
/// data to [`decode_data_with_resolver`].
///
/// # Errors
///
/// Returns [`FilterError`] if any filter in the chain fails or is unsupported.
pub fn decode_with_resolver<'a>(
    stream: &'a StreamObject,
    objects: &dyn ObjectResolver,
) -> Result<Cow<'a, [u8]>, FilterError> {
    decode_data_with_resolver(&stream.dictionary, stream.raw_data(), objects)
}

/// Decodes a [`StreamObject`] by applying its full filter chain.
///
/// This convenience wrapper uses a passthrough resolver, so it only supports
/// direct `/Filter` and `/DecodeParms` values.
pub fn decode(stream: &StreamObject) -> Result<Cow<'_, [u8]>, FilterError> {
    let objects = PassthroughResolver;
    decode_with_resolver(stream, &objects)
}

/// Parses the `/DecodeParms` entry from a stream dictionary into a
/// [`Vec<DecodeParms>`] aligned 1-to-1 with `filters`.
///
/// Per PDF spec §7.3.8.2, `/DecodeParms` is either a single dictionary (when
/// there is one filter) or an array of dictionaries (one per filter).
fn parse_decode_params(
    dict: &Dictionary,
    filters: &[Filter],
    objects: &dyn ObjectResolver,
) -> Result<Vec<DecodeParms>, FilterError> {
    let param_dicts = resolve_decode_parms_dicts(dict, filters, objects)?;

    filters
        .iter()
        .zip(param_dicts.iter().copied())
        .map(|(filter, param_dict)| decode_parms_for_filter(filter, param_dict, objects))
        .collect()
}

fn resolve_decode_parms_dicts<'a>(
    dict: &'a Dictionary,
    filters: &[Filter],
    objects: &'a dyn ObjectResolver,
) -> Result<Vec<Option<&'a Dictionary>>, FilterError> {
    let Some(entry) = dict.get("DecodeParms") else {
        return Ok(vec![None; filters.len()]);
    };

    let resolved = objects.resolve_object(entry)?;

    match resolved {
        ObjectVariant::Dictionary(d) => Ok(vec![Some(d.as_ref()); filters.len()]),
        ObjectVariant::Array(arr) => {
            resolve_decode_parms_array(arr.as_slice(), filters.len(), objects)
        }
        other => Err(FilterError::from(
            pdf_object::error::ObjectError::TypeMismatch("Dictionary or Array", other.name()),
        )),
    }
}

fn resolve_decode_parms_array<'a>(
    arr: &'a [ObjectVariant],
    filter_count: usize,
    objects: &'a dyn ObjectResolver,
) -> Result<Vec<Option<&'a Dictionary>>, FilterError> {
    (0..filter_count)
        .map(|index| {
            let Some(item) = arr.get(index) else {
                return Ok(None);
            };

            let resolved = objects.resolve_object(item)?;
            match resolved {
                ObjectVariant::Dictionary(d) => Ok(Some(d.as_ref())),
                ObjectVariant::Null => Ok(None),
                other => Err(FilterError::from(
                    pdf_object::error::ObjectError::TypeMismatch("Dictionary", other.name()),
                )),
            }
        })
        .collect()
}

fn decode_parms_for_filter(
    filter: &Filter,
    param_dict: Option<&Dictionary>,
    objects: &dyn ObjectResolver,
) -> Result<DecodeParms, FilterError> {
    let params = match (filter, param_dict) {
        (Filter::CCITTFaxDecode, Some(d)) => {
            let p = CCITTFaxParams::from_dictionary(d, objects)?;
            DecodeParms::CcittFax(p)
        }
        (Filter::CCITTFaxDecode, None) => DecodeParms::CcittFax(CCITTFaxParams::default()),
        (Filter::LZWDecode, Some(d)) => {
            let early_change = d.optional_number("EarlyChange", objects)?.unwrap_or(1) != 0;
            let predictor = PredictorParams::from_dictionary(d, objects)?;
            DecodeParms::Lzw {
                early_change,
                predictor,
            }
        }
        (Filter::LZWDecode, None) => DecodeParms::Lzw {
            early_change: true,
            predictor: PredictorParams::default(),
        },
        (Filter::FlateDecode, Some(d)) => {
            let predictor = PredictorParams::from_dictionary(d, objects)?;
            DecodeParms::Flate { predictor }
        }
        (Filter::FlateDecode, None) => DecodeParms::Flate {
            predictor: PredictorParams::default(),
        },
        (Filter::JBIG2Decode, Some(d)) => DecodeParms::Jbig2 {
            globals: resolve_jbig2_globals(d, objects)?,
        },
        (Filter::JBIG2Decode, None) => DecodeParms::Jbig2 { globals: None },
        _ => DecodeParms::None,
    };

    Ok(params)
}

fn resolve_jbig2_dimensions(
    dict: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<(u16, u16), FilterError> {
    let missing_dimensions_error =
        || FilterError::Decompression("JBIG2Decode requires positive Width and Height".into());

    let width = dict
        .optional_number::<u16>("Width", objects)?
        .ok_or_else(missing_dimensions_error)?;
    let height = dict
        .optional_number::<u16>("Height", objects)?
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
    let Some(globals_stream) = dict.optional_stream("JBIG2Globals", objects)? else {
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
        let filter = Filter::from("ASCIIHexDecode");
        assert_eq!(filter, Filter::ASCIIHexDecode);
        assert_eq!(filter.to_string(), "ASCIIHexDecode");
    }

    #[test]
    fn test_filter_name_round_trip_run_length() {
        let filter = Filter::from("RunLengthDecode");
        assert_eq!(filter, Filter::RunLengthDecode);
        assert_eq!(filter.to_string(), "RunLengthDecode");
    }

    #[test]
    fn test_filter_name_round_trip_jbig2() {
        let filter = Filter::from("JBIG2Decode");
        assert_eq!(filter, Filter::JBIG2Decode);
        assert_eq!(filter.to_string(), "JBIG2Decode");
    }

    #[test]
    fn decode_data_without_filters_borrows_input() {
        let dictionary = Dictionary::new(BTreeMap::new());
        let data = b"borrowed stream data";

        let decoded = decode_data_with_resolver(&dictionary, data, &PassthroughResolver)
            .expect("unfiltered data should decode");

        assert!(matches!(&decoded, Cow::Borrowed(_)));
        assert_eq!(decoded.as_ref().as_ptr(), data.as_ptr());
        assert_eq!(decoded.as_ref(), data);
    }

    #[test]
    fn decode_data_with_filter_returns_decoded_owned_data() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            "Filter".to_string(),
            ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
        )]));

        let decoded =
            decode_data_with_resolver(&dictionary, b"48 65 6c 6c 6f>", &PassthroughResolver)
                .expect("filtered data should decode");

        assert!(matches!(&decoded, Cow::Owned(_)));
        assert_eq!(decoded.as_ref(), b"Hello");
    }

    #[test]
    fn test_jbig2_missing_dimensions_returns_decode_error() {
        let mut dict = BTreeMap::new();
        dict.insert(
            "Filter".to_string(),
            ObjectVariant::Name(b"JBIG2Decode".to_vec()),
        );

        let stream = StreamObject::new(1, 0, Box::new(Dictionary::new(dict)), Vec::new());
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
            "Filter".to_string(),
            ObjectVariant::Name(b"JBIG2Decode".to_vec()),
        );
        let globals_stream =
            StreamObject::new(3, 0, Box::new(Dictionary::new(globals_dict)), Vec::new());

        let mut decode_parms = BTreeMap::new();
        decode_parms.insert("JBIG2Globals".to_string(), ObjectVariant::Reference(3));

        let mut dict = BTreeMap::new();
        dict.insert(
            "Filter".to_string(),
            ObjectVariant::Name(b"JBIG2Decode".to_vec()),
        );
        dict.insert("Width".to_string(), ObjectVariant::Integer(8));
        dict.insert("Height".to_string(), ObjectVariant::Integer(1));
        dict.insert("DecodeParms".to_string(), ObjectVariant::Reference(2));

        let stream = StreamObject::new(1, 0, Box::new(Dictionary::new(dict)), Vec::new());

        let mut objects = BTreeMap::new();
        objects.insert(
            2,
            ObjectVariant::Dictionary(Box::new(Dictionary::new(decode_parms))),
        );
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
            "Filter".to_string(),
            ObjectVariant::Name(b"JBIG2Decode".to_vec()),
        );
        dict.insert("Width".to_string(), ObjectVariant::Integer(8));
        dict.insert("Height".to_string(), ObjectVariant::Integer(1));

        let stream = StreamObject::new(1, 0, Box::new(Dictionary::new(dict)), vec![0x00]);

        let err = decode(&stream).expect_err("expected decode failure");
        assert!(matches!(err, FilterError::Decompression(_)));
    }

    #[test]
    fn test_decode_ascii_hex_stream() {
        let mut dict = BTreeMap::new();
        dict.insert(
            "Filter".to_string(),
            ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
        );

        let stream = StreamObject::new(
            1,
            0,
            Box::new(Dictionary::new(dict)),
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
        dict.insert("Filter".to_string(), ObjectVariant::Reference(3));
        dict.insert(
            "Length".to_string(),
            ObjectVariant::Integer(compressed.len() as i64),
        );

        let stream = StreamObject::new(1, 0, Box::new(Dictionary::new(dict)), compressed);

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
    fn test_decode_run_length_stream() {
        let mut dict = BTreeMap::new();
        dict.insert(
            "Filter".to_string(),
            ObjectVariant::Name(b"RunLengthDecode".to_vec()),
        );

        let stream = StreamObject::new(
            1,
            0,
            Box::new(Dictionary::new(dict)),
            vec![2, b'A', b'B', b'C', 255, b'!', 128],
        );

        let decoded = decode(&stream).expect("decode failed");
        assert_eq!(decoded.as_ref(), b"ABC!!");
    }

    #[test]
    fn test_decode_rl_alias_stream() {
        let mut dict = BTreeMap::new();
        dict.insert("Filter".to_string(), ObjectVariant::Name(b"RL".to_vec()));

        let stream = StreamObject::new(1, 0, Box::new(Dictionary::new(dict)), vec![0, b'X', 128]);

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
