use std::borrow::Cow;
use std::fmt;

use crate::{ccitt_fax_params::CCITTFaxParams, error::FilterError, predictor::PredictorParams};

use pdf_object::{
    dictionary::Dictionary,
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
    /// A filter that is not currently supported by this implementation.
    ///
    /// The contained string holds the original filter name from the PDF,
    /// allowing for future expansion or debugging purposes.
    Unsupported(String),
}

impl From<Cow<'_, str>> for Filter {
    fn from(name: Cow<'_, str>) -> Self {
        match name.as_ref() {
            "DCTDecode" => Self::DCTDecode,
            "FlateDecode" => Self::FlateDecode,
            "JPXDecode" => Self::JPXDecode,
            "CCITTFaxDecode" => Self::CCITTFaxDecode,
            "ASCII85Decode" => Self::ASCII85Decode,
            "ASCIIHexDecode" => Self::ASCIIHexDecode,
            "LZWDecode" => Self::LZWDecode,
            _ => Self::Unsupported(name.into_owned()),
        }
    }
}

impl From<&str> for Filter {
    fn from(name: &str) -> Self {
        Self::from(Cow::Borrowed(name))
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

        let data = match pixels.data {
            jpeg2k::ImagePixelData::L8(data) => data,
            jpeg2k::ImagePixelData::Rgb16(data) => data
                .into_iter()
                .flat_map(|v| v.to_be_bytes())
                .collect::<Vec<u8>>(),
            jpeg2k::ImagePixelData::L16(data) => data
                .into_iter()
                .flat_map(|v| v.to_be_bytes())
                .collect::<Vec<u8>>(),
            _ => {
                return Err(FilterError::Decompression(
                    "unsupported JPEG 2000 pixel format".to_string(),
                ));
            }
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

/// Decodes a [`StreamObject`] by applying its full filter chain.
///
/// Reads the `/Filter` entry from the stream's dictionary and applies each
/// filter in order. Returns the fully decoded bytes, or `Cow::Borrowed` if
/// no filters are present.
///
/// This is the main entry point for stream decoding in the `pdf-filter` crate.
///
/// # Errors
///
/// Returns [`FilterError`] if any filter in the chain fails or is unsupported.
pub fn decode(stream: &StreamObject) -> Result<Cow<'_, [u8]>, FilterError> {
    let mut data: Cow<'_, [u8]> = Cow::Borrowed(&stream.data);
    let objects = PassthroughResolver;
    let filters = Filter::from_dictionary(&stream.dictionary, &objects)?;

    let Some(filters) = &filters else {
        return Ok(data);
    };

    let decode_params = parse_decode_params(&stream.dictionary, filters, &objects);

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
            Filter::CCITTFaxDecode => {
                let ccitt_params = match params {
                    DecodeParms::CcittFax(p) => p,
                    _ => &CCITTFaxParams::DEFAULT,
                };
                let decoded = crate::ccitt::decode(&data, ccitt_params)?;
                data = Cow::Owned(decoded);
            }
            Filter::Unsupported(name) => {
                return Err(FilterError::UnsupportedFilter(name.clone()));
            }
        }
    }
    Ok(data)
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
) -> Vec<DecodeParms> {
    let params_entry = dict.get("DecodeParms");

    // Pre-extract per-index dictionaries from the `/DecodeParms` value.
    let param_dicts: Vec<Option<&Dictionary>> = match params_entry {
        Some(ObjectVariant::Dictionary(d)) => {
            // Single dict applies to all filters (common single-filter case).
            vec![Some(d); filters.len()]
        }
        Some(ObjectVariant::Array(arr)) => filters
            .iter()
            .enumerate()
            .map(|(i, _)| {
                arr.get(i).and_then(|v| {
                    if let ObjectVariant::Dictionary(d) = v {
                        Some(d.as_ref())
                    } else {
                        None
                    }
                })
            })
            .collect(),
        _ => vec![None; filters.len()],
    };

    filters
        .iter()
        .zip(param_dicts.iter())
        .map(|(filter, param_dict)| match (filter, param_dict) {
            (Filter::CCITTFaxDecode, Some(d)) => {
                let p = CCITTFaxParams::from_dictionary(d, objects).unwrap_or_default();
                DecodeParms::CcittFax(p)
            }
            (Filter::CCITTFaxDecode, None) => DecodeParms::CcittFax(CCITTFaxParams::default()),
            (Filter::LZWDecode, Some(d)) => {
                let early_change = d
                    .get("EarlyChange")
                    .and_then(|v| v.try_number::<i64>(objects).ok())
                    .unwrap_or(1)
                    != 0;
                let predictor = PredictorParams::from_dictionary(d, objects).unwrap_or_default();
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
                let predictor = PredictorParams::from_dictionary(d, objects).unwrap_or_default();
                DecodeParms::Flate { predictor }
            }
            (Filter::FlateDecode, None) => DecodeParms::Flate {
                predictor: PredictorParams::default(),
            },
            _ => DecodeParms::None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, collections::BTreeMap};

    use pdf_object::{dictionary::Dictionary, object_variant::ObjectVariant, stream::StreamObject};

    use super::*;

    #[test]
    fn test_filter_name_round_trip_ascii_hex() {
        let filter = Filter::from(Cow::Borrowed("ASCIIHexDecode"));
        assert_eq!(filter, Filter::ASCIIHexDecode);
        assert_eq!(filter.to_string(), "ASCIIHexDecode");
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
}
