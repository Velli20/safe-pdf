use std::borrow::Cow;

use crate::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    traits::FromDictionary,
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
            _ => Self::Unsupported(name.into_owned()),
        }
    }
}

impl From<&str> for Filter {
    fn from(name: &str) -> Self {
        Self::from(Cow::Borrowed(name))
    }
}

/// Represents the compression filter applied to an image's stream data.
///
/// This corresponds to the `/Filter` entry in a PDF Image XObject's dictionary.
/// The filter specifies the algorithm used to decompress the raw image data.
impl FromDictionary for Filter {
    const KEY: &'static str = "Filter";

    type ResultType = Option<Vec<Filter>>;
    type ErrorType = ObjectError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self::ResultType, Self::ErrorType> {
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

impl Filter {
    /// Decodes FlateDecode (zlib/deflate) compressed stream data.
    ///
    /// # Parameters
    ///
    /// - `stream_data`: The compressed byte stream to decode.
    ///
    /// # Returns
    ///
    /// The decompressed data as a `Vec<u8>`, or an error if decompression fails.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectError::DecompressionError`] if the zlib decompression fails,
    /// which can happen if the data is corrupted or not valid zlib-compressed data.
    pub fn decode_flate(stream_data: &[u8]) -> Result<Vec<u8>, ObjectError> {
        let mut decoder = flate2::read::ZlibDecoder::new(stream_data);
        let mut decoded = Vec::new();

        use std::io::Read;

        if let Err(e) = decoder.read_to_end(&mut decoded) {
            return Err(ObjectError::DecompressionError(e.to_string()));
        }

        Ok(decoded)
    }

    /// Decodes JPXDecode (JPEG 2000) compressed stream data.
    ///
    /// # Parameters
    ///
    /// - `stream_data`: The JPEG 2000 compressed byte stream to decode.
    ///
    /// # Returns
    ///
    /// The decompressed image data as a `Vec<u8>`, or an error if decompression fails.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectError::DecompressionError`] if the JPEG 2000 decoding fails,
    /// which can happen if the data is corrupted or not valid JPEG 2000 data.
    pub fn decode_jpeg2000(stream_data: &[u8]) -> Result<Vec<u8>, ObjectError> {
        let bitmap = jpeg2k::Image::from_bytes(stream_data)
            .map_err(|e| ObjectError::DecompressionError(e.to_string()))?;

        let pixels = bitmap
            .get_pixels(None)
            .map_err(|e| ObjectError::DecompressionError(e.to_string()))?;

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
                return Err(ObjectError::DecompressionError(
                    "Unsupported JPEG 2000 pixel format".to_string(),
                ));
            }
        };

        Ok(data)
    }

    /// Decodes DCTDecode (JPEG) compressed stream data.
    ///
    /// # Parameters
    ///
    /// - `stream_data`: The JPEG compressed byte stream to decode.
    ///
    /// # Returns
    ///
    /// The decompressed image data as a `Vec<u8>`, or an error if decompression fails.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectError::DecompressionError`] if the JPEG decoding fails,
    /// which can happen if the data is corrupted or not a valid JPEG image.
    pub fn decode_jpeg_baseline(stream_data: &[u8]) -> Result<Vec<u8>, ObjectError> {
        let bitmap = image::load_from_memory_with_format(stream_data, image::ImageFormat::Jpeg)
            .map_err(|e| ObjectError::DecompressionError(e.to_string()))?;

        Ok(bitmap.as_bytes().to_vec())
    }

    pub fn decode_ccitt_fax(_stream_data: &[u8]) -> Result<Vec<u8>, ObjectError> {
        Err(ObjectError::DecompressionError(
            "CCITT Fax decoding not implemented".to_string(),
        ))
    }
}
