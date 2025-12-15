//! PDF Image Filter parsing and representation.
//!
//! This module provides types for working with PDF image stream filters,
//! which specify the compression algorithms used to encode image data.

use pdf_object::{ObjectVariant, error::ObjectError, traits::FromDictionary};
use std::borrow::Cow;
use std::io::Read;
use thiserror::Error;

/// Represents the compression filter applied to an image's stream data.
///
/// This corresponds to the `/Filter` entry in a PDF Image XObject's dictionary.
/// The filter specifies the algorithm used to decompress the raw image data.
///
/// # PDF Reference
///
/// See PDF Reference 1.7, Section 7.4 "Filters" for the complete list of
/// standard filters and their specifications.
///
/// # Example
///
/// ```ignore
/// use pdf_page::image_filter::ImageFilter;
/// use std::borrow::Cow;
///
/// let filter = ImageFilter::from(Cow::Borrowed("DCTDecode"));
/// assert_eq!(filter, ImageFilter::DCTDecode);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageFilter {
    /// The DCT (Discrete Cosine Transform) filter, used for JPEG-compressed images.
    ///
    /// This is a lossy compression algorithm commonly used for photographic images.
    /// The decompressed data is typically in a format suitable for direct display.
    DCTDecode,
    /// The Flate (zlib/deflate) filter, a lossless compression algorithm.
    ///
    /// Based on the zlib/deflate algorithm (RFC 1950, RFC 1951), this is one of
    /// the most commonly used filters in PDF for general-purpose compression.
    FlateDecode,
    /// A filter that is not currently supported by this implementation.
    ///
    /// The contained string holds the original filter name from the PDF,
    /// allowing for future expansion or debugging purposes.
    Unsupported(String),
    /// A chain of multiple filters applied in sequence.
    ///
    /// The filters are applied in the order they appear in the vector.
    Chained(Vec<ImageFilter>),
}

/// Errors that can occur when parsing PDF image filters.
#[derive(Debug, Error)]
pub enum ImageFilterError {
    /// An error occurred while resolving PDF objects.
    #[error("failed to resolve PDF object: {0}")]
    ObjectResolution(#[from] ObjectError),
    /// The filter value has an unexpected type (expected Name or Array of Names).
    #[error("invalid filter type: expected Name or Array, got {found}")]
    InvalidFilterType {
        /// Description of the actual type found.
        found: String,
    },
    /// Failed to decode the compressed image stream.
    #[error("Failed to decode image stream: {reason}")]
    StreamDecodeError {
        /// A description of why decoding failed.
        reason: String,
    },
    /// The image uses a filter that is not supported by this implementation.
    #[error("Unsupported filter: '{name}'")]
    UnsupportedFilter {
        /// The name of the unsupported filter.
        name: String,
    },
}

impl From<Cow<'_, str>> for ImageFilter {
    fn from(name: Cow<'_, str>) -> Self {
        match name.as_ref() {
            "DCTDecode" => Self::DCTDecode,
            "FlateDecode" => Self::FlateDecode,
            _ => Self::Unsupported(name.into_owned()),
        }
    }
}

impl From<&str> for ImageFilter {
    fn from(name: &str) -> Self {
        Self::from(Cow::Borrowed(name))
    }
}

impl FromDictionary for ImageFilter {
    const KEY: &'static str = "Filter";

    type ResultType = Option<ImageFilter>;
    type ErrorType = ImageFilterError;

    fn from_dictionary(
        dictionary: &pdf_object::dictionary::Dictionary,
        objects: &pdf_object::object_collection::ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let Some(filter_obj) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        let resolved = objects.resolve_object(filter_obj)?;

        // Parse the `/Filter` entry: can be either a single Name or an Array of Names.
        // Per PDF spec, filters are applied in order when multiple are present.
        let filters = match resolved {
            ObjectVariant::Array(arr) => {
                let filters = arr
                    .iter()
                    .map(|item| {
                        item.try_str()
                            .map(ImageFilter::from)
                            .map_err(ImageFilterError::from)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                ImageFilter::Chained(filters)
            }
            ObjectVariant::Name(name) => ImageFilter::from(name.as_str()),
            other => {
                // Handle single name that wasn't parsed as Name variant
                match other.try_str() {
                    Ok(name) => ImageFilter::from(name),
                    Err(_) => {
                        return Err(ImageFilterError::InvalidFilterType {
                            found: other.name().to_string(),
                        });
                    }
                }
            }
        };

        Ok(Some(filters))
    }
}

impl ImageFilter {
    /// Applies the filter chain to the given data.
    ///
    /// PDF images can have multiple filters applied in sequence. This method handles:
    /// - Non-chained filters (returned for later processing)
    /// - Any number of leading `FlateDecode` filters (decompressed immediately)
    /// - Multi-stage patterns like `[/FlateDecode, /DCTDecode]` (common for optimized JPEGs)
    ///
    /// All leading `FlateDecode` filters are applied immediately, and the remaining
    /// filter (if any) is returned for the caller to handle.
    ///
    /// # Returns
    ///
    /// The remaining filter after processing all `FlateDecode` stages.
    pub(crate) fn apply(&self, data: &mut Vec<u8>) -> Result<ImageFilter, ImageFilterError> {
        let ImageFilter::Chained(filters) = self else {
            return Ok(self.clone());
        };

        match filters.as_slice() {
            // Two-stage compression: FlateDecode followed by another filter.
            // This is a common optimization pattern in PDFs.
            [ImageFilter::FlateDecode, second_filter] => {
                // Apply FlateDecode decompression immediately.
                *data = decode_flate(data)?;
                // Return the second filter for later processing.
                Ok(second_filter.clone())
            }
            [ImageFilter::FlateDecode] => {
                // Single FlateDecode filter: decompress the data.
                *data = decode_flate(data)?;
                Ok(ImageFilter::FlateDecode)
            }
            [single_filter] => Ok(single_filter.clone()),
            // More than two filters are not supported.
            _ => Err(ImageFilterError::UnsupportedFilter {
                name: format!(
                    "Filter chains with {} filters are not supported",
                    filters.len()
                ),
            }),
        }
    }
}

/// Decodes FlateDecode (zlib/deflate) compressed stream data.
///
/// # Arguments
///
/// * `stream_data` - The compressed byte stream to decode.
///
/// # Returns
///
/// The decompressed data as a `Vec<u8>`, or an error if decompression fails.
///
/// # Errors
///
/// Returns [`ImageFilterError::StreamDecodeError`] if the zlib decompression fails,
/// which can happen if the data is corrupted or not valid zlib-compressed data.
fn decode_flate(stream_data: &[u8]) -> Result<Vec<u8>, ImageFilterError> {
    let mut decoder = flate2::read::ZlibDecoder::new(stream_data);
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .map_err(|e| ImageFilterError::StreamDecodeError {
            reason: e.to_string(),
        })?;
    Ok(decoded)
}
