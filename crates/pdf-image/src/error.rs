use pdf_color_space::error::ColorSpaceError;
use pdf_decode::DecodeError;
use pdf_filter::error::FilterError;
use pdf_jpeg2000::Jpeg2000Error;
use thiserror::Error;

/// Errors that can occur while parsing or decoding PDF image data.
#[derive(Error, Debug)]
pub enum PdfImageError {
    #[error("{0}")]
    Object(#[from] pdf_object_reader::object_error::ObjectError),
    #[error("{0}")]
    Read(#[from] pdf_object_reader::ObjectReadError),
    #[error("{0}")]
    ColorSpace(#[from] ColorSpaceError),
    #[error("{0}")]
    Filter(#[from] FilterError),
    #[error("JPEG 2000 image data could not be decoded: {0}")]
    Jpx(#[from] Jpeg2000Error),
    #[error("JPEG 2000 image is too large to store: {pixels} pixels across {channels} channels")]
    JpxImageTooLarge { pixels: usize, channels: usize },
    #[error("inline images may not use the JPXDecode filter")]
    InlineJpxFilter,
    #[error("unsupported /SMaskInData value: {value} (supported: 0, 1, 2)")]
    UnsupportedSMaskInData { value: i64 },
    #[error("invalid soft mask XObject: /SMask must reference an image XObject")]
    InvalidSoftMaskXObject,
    #[error("invalid image dimensions: width={width}, height={height}")]
    InvalidImageDimensions { width: usize, height: usize },
    #[error(
        "unsupported image BitsPerComponent value: {bits_per_component} (supported: 1, 2, 4, 8, 16)"
    )]
    UnsupportedImageBitsPerComponent { bits_per_component: usize },
    #[error(
        "unsupported indexed BitsPerComponent value: {bits_per_component} (supported: 1, 2, 4, 8)"
    )]
    UnsupportedIndexedBits { bits_per_component: usize },
    #[error("{0}")]
    InvalidImageData(String),
    #[error("invalid image color space: reported zero color components")]
    InvalidColorComponentCount,
    #[error("invalid /Decode array length: expected {expected_values} values, got {actual_values}")]
    InvalidDecodeLength {
        expected_values: usize,
        actual_values: usize,
    },
    #[error("invalid /Decode value")]
    InvalidDecodeValue,
    #[error("truncated image data: expected at least {expected_bytes} bytes, got {actual_bytes}")]
    TruncatedImageData {
        expected_bytes: usize,
        actual_bytes: usize,
    },
}

impl PdfImageError {
    /// Returns whether the image's stream data could not be decoded or is too
    /// short for its dimensions, so the image has no samples to draw.
    pub fn is_unreadable_data(&self) -> bool {
        matches!(self, Self::Filter(_) | Self::TruncatedImageData { .. })
    }
}

impl From<DecodeError> for PdfImageError {
    fn from(value: DecodeError) -> Self {
        match value {
            DecodeError::Object(err) => Self::Object(err),
            DecodeError::InvalidBitsPerSample { bits_per_sample } => {
                Self::UnsupportedImageBitsPerComponent {
                    bits_per_component: bits_per_sample,
                }
            }
            DecodeError::InsufficientData {
                expected_bytes,
                actual_bytes,
            } => Self::TruncatedImageData {
                expected_bytes,
                actual_bytes,
            },
            DecodeError::InvalidDecodeLength {
                expected_values,
                actual_values,
            } => Self::InvalidDecodeLength {
                expected_values,
                actual_values,
            },
            DecodeError::InvalidDecodeValue => Self::InvalidDecodeValue,
            DecodeError::InvalidComponentCount => Self::InvalidColorComponentCount,
            DecodeError::PaletteLookupOutOfBounds {
                index,
                pixel_index,
                lookup_len,
            } => Self::InvalidImageData(format!(
                "Palette index {index} out of bounds at pixel {pixel_index} (lookup table size: {lookup_len})"
            )),
            DecodeError::InvalidSampleData => {
                Self::InvalidImageData("packed sample value cannot fit in a byte".to_string())
            }
        }
    }
}

/// Failures while validating or processing render-ready image pixels.
#[derive(Debug, Error)]
pub enum ImageRasterError {
    /// Dimensions, packed pixel data, or computed alpha are invalid.
    #[error("invalid image raster input: {0}")]
    InvalidInput(&'static str),
    /// The requested buffer size overflows or its allocation fails.
    #[error("image raster allocation limit exceeded")]
    ResourceLimit,
}
