use pdf_color_space::error::ColorSpaceError;
use pdf_filter::error::FilterError;
use pdf_object::error::ObjectError;
use thiserror::Error;

/// Errors that can occur while parsing or decoding PDF image data.
#[derive(Error, Debug)]
pub enum PdfImageError {
    #[error("{0}")]
    Object(#[from] ObjectError),
    #[error("{0}")]
    ColorSpace(#[from] ColorSpaceError),
    #[error("{0}")]
    Filter(#[from] FilterError),
    #[error("invalid soft mask XObject: /SMask must reference an image XObject")]
    InvalidSoftMaskXObject,
    #[error("invalid image dimensions: width={width}, height={height}")]
    InvalidImageDimensions { width: usize, height: usize },
    #[error("unsupported image BitsPerComponent value: {bits_per_component} (supported: 1, 8)")]
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
    pub fn is_cyclic_dependency(&self) -> bool {
        matches!(self, Self::Object(ObjectError::CyclicDependency { .. }))
    }
}
