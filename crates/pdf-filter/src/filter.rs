use std::borrow::Cow;
use std::fmt;

use crate::{ccitt::CcittDecodeError, ccitt_fax_params::CCITTFaxParams, error::FilterError};

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
            Self::Unsupported(name) => f.write_str(name),
        }
    }
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
    pub fn decode_flate(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
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
    pub fn decode_jpeg2000(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
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
    pub fn decode_jpeg_baseline(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
        let bitmap = image::load_from_memory_with_format(stream_data, image::ImageFormat::Jpeg)
            .map_err(|e| FilterError::Decompression(e.to_string()))?;

        Ok(bitmap.as_bytes().to_vec())
    }

    /// Decodes CCITTFaxDecode (Group 3 / Group 4 fax) compressed stream data.
    ///
    /// # Errors
    ///
    /// Returns [`CcittDecodeError`] if the stream is truncated or
    /// contains an invalid bit pattern.
    pub fn decode_ccitt_fax(
        stream_data: &[u8],
        params: &CCITTFaxParams,
    ) -> Result<Vec<u8>, CcittDecodeError> {
        crate::ccitt::decode(stream_data, params)
    }

    /// Decodes ASCII85Decode-encoded stream data.
    ///
    /// ASCII85 encodes 4 binary bytes as 5 printable ASCII characters in the
    /// range `!` (33) through `u` (117), using base 85. The special symbol `z`
    /// represents a group of four zero bytes. Whitespace is ignored. The
    /// end-of-data marker `~>` terminates the stream; any bytes after it are
    /// discarded.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Decompression`] if an invalid character is
    /// encountered (outside the expected ASCII85 alphabet).
    pub fn decode_ascii85(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
        let mut output = Vec::with_capacity(stream_data.len().saturating_div(5).saturating_mul(4));
        let mut group = [0u32; 5];
        let mut group_len = 0usize;

        for &byte in stream_data {
            // End-of-data marker `~>`
            if byte == b'~' {
                break;
            }

            // Skip whitespace
            if byte.is_ascii_whitespace() {
                continue;
            }

            // `z` shorthand: four zero bytes (only valid at a group boundary)
            if byte == b'z' {
                if group_len != 0 {
                    return Err(FilterError::Decompression(
                        "ASCII85: 'z' encountered in the middle of a group".to_string(),
                    ));
                }
                output.extend_from_slice(&[0u8; 4]);
                continue;
            }

            if !(b'!'..=b'u').contains(&byte) {
                return Err(FilterError::Decompression(format!(
                    "ASCII85: invalid character 0x{byte:02X}"
                )));
            }

            if let Some(slot) = group.get_mut(group_len) {
                // byte is validated to be in b'!'..=b'u', so wrapping_sub is safe
                *slot = u32::from(byte.wrapping_sub(b'!'));
            }
            group_len = group_len.saturating_add(1);

            if group_len == 5 {
                // Horner's method: avoids large intermediate exponents
                let val = group
                    .iter()
                    .fold(0u32, |acc, &d| acc.wrapping_mul(85).wrapping_add(d));
                output.extend_from_slice(&val.to_be_bytes());
                group_len = 0;
            }
        }

        // Handle the final partial group (1–4 chars → 1–3 bytes)
        if group_len > 0 {
            let partial_bytes = group_len.saturating_sub(1);
            // Pad remaining slots with `u` (value 84) per PDF spec §7.4.3
            for slot in group.iter_mut().skip(group_len) {
                *slot = 84;
            }
            let val = group
                .iter()
                .fold(0u32, |acc, &d| acc.wrapping_mul(85).wrapping_add(d));
            let bytes = val.to_be_bytes();
            if let Some(slice) = bytes.get(..partial_bytes) {
                output.extend_from_slice(slice);
            }
        }

        Ok(output)
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
    let filters = Filter::from_dictionary(&stream.dictionary, &PassthroughResolver)?;

    let Some(filters) = &filters else {
        return Ok(data);
    };

    for (filter_idx, filter) in filters.iter().enumerate() {
        match filter {
            Filter::FlateDecode => {
                let decoded = Filter::decode_flate(&data)?;
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
                let decoded = Filter::decode_ascii85(&data)?;
                data = Cow::Owned(decoded);
            }
            Filter::CCITTFaxDecode => {
                let objects = PassthroughResolver;
                let params = ccitt_params_for_filter(&stream.dictionary, filter_idx, &objects);
                let decoded = Filter::decode_ccitt_fax(&data, &params)?;
                data = Cow::Owned(decoded);
            }
            Filter::Unsupported(name) => {
                return Err(FilterError::UnsupportedFilter(name.clone()));
            }
        }
    }
    Ok(data)
}

/// Extract [`CCITTFaxParams`][crate::ccitt::CCITTFaxParams] for the filter at
/// `filter_idx` from the stream's `/DecodeParms` dictionary entry.
///
/// Per PDF spec §7.3.8.2, `/DecodeParms` is either a single dictionary (when
/// there is one filter) or an array of dictionaries (one per filter). Values
/// are always inline objects, so no object resolver is needed.
fn ccitt_params_for_filter(
    dict: &Dictionary,
    filter_idx: usize,
    objects: &dyn ObjectResolver,
) -> CCITTFaxParams {
    match dict.get("DecodeParms") {
        Some(ObjectVariant::Dictionary(d)) => {
            CCITTFaxParams::from_dictionary(d, objects).unwrap_or_default()
        }
        Some(ObjectVariant::Array(arr)) => arr
            .get(filter_idx)
            .and_then(|v| {
                if let ObjectVariant::Dictionary(d) = v {
                    Some(CCITTFaxParams::from_dictionary(d, objects).unwrap_or_default())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        _ => CCITTFaxParams::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_ascii85_basic() {
        // "Man " → known ASCII85 encoding "9jqo^"
        let man_encoded = b"9jqo^~>";
        let decoded = Filter::decode_ascii85(man_encoded).expect("decode failed");
        assert_eq!(decoded, b"Man ");
    }

    #[test]
    fn test_decode_ascii85_z_shorthand() {
        // `z` represents 4 zero bytes
        let input = b"z~>";
        let decoded = Filter::decode_ascii85(input).expect("decode failed");
        assert_eq!(decoded, [0u8; 4]);
    }

    #[test]
    fn test_decode_ascii85_multiple_groups() {
        // "Man is di" — two full groups + one partial group (1 byte → 2 chars)
        let input = b"9jqo^BlbD-B`~>";
        let decoded = Filter::decode_ascii85(input).expect("decode failed");
        assert_eq!(&decoded, b"Man is di");
    }

    #[test]
    fn test_decode_ascii85_whitespace_ignored() {
        // Whitespace (spaces, newlines) must be ignored
        let input = b"9j qo\n^~>";
        let decoded = Filter::decode_ascii85(input).expect("decode failed");
        assert_eq!(&decoded, b"Man ");
    }

    #[test]
    fn test_decode_ascii85_partial_group() {
        // 2 input chars → 1 output byte
        // Encode 0xAB: pad to [0xAB, 84<<24…] etc.
        // 0xAB000000 in base85: 0xAB000000 = 2,869,231,616
        // / 85^4 = 2869231616 / 52200625 = 54 → char '!'+ 54 = 'W'
        // remainder: 2869231616 - 54*52200625 = 2869231616 - 2818833750 = 50397866
        // / 85^3 = 50397866 / 614125 = 82 → char '!'+ 82 = 's'
        // So first two chars of the full 5-char group for 0xAB000000 are "Ws..."
        // We only need the first 2 chars to recover 1 byte.
        let input = b"Ws~>";
        let decoded = Filter::decode_ascii85(input).expect("decode failed");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0], 0xAB);
    }

    #[test]
    fn test_decode_ascii85_invalid_char() {
        let input = b"9jqo\x80~>";
        let result = Filter::decode_ascii85(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_ascii85_z_in_middle_of_group_is_error() {
        // 'z' mid-group is invalid
        let input = b"9jz~>";
        let result = Filter::decode_ascii85(input);
        assert!(result.is_err());
    }
}
