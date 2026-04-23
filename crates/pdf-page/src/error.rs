use pdf_color_space::error::ColorSpaceError;
use pdf_content_stream::error::PdfOperatorError;
use pdf_font::error::FontError;
use pdf_function::{
    error::FunctionReadError, function_interpolation_error::FunctionInterpolationError,
};

use pdf_object::error::ObjectError;
use thiserror::Error;

/// Errors that can occur during parsing of a PDF Pages object.
#[derive(Error, Debug)]
pub enum PdfPagesError {
    #[error("invalid /Kids entry type: expected /Page or /Pages, found '{found_type}'")]
    InvalidKidsEntryType { found_type: String },
    #[error("{0}")]
    Object(#[from] ObjectError),
    #[error("failed to parse content stream: {0}")]
    ContentStream(#[from] PdfOperatorError),
    #[error("{0}")]
    ColorSpace(#[from] ColorSpaceError),
    #[error("{0}")]
    FunctionInterpolation(#[from] FunctionInterpolationError),
    #[error("failed to process font: {0}")]
    Font(#[from] FontError),
    #[error(
        "invalid /ExtGState entry '/{entry}': expected {expected_structure}, found {actual_structure}"
    )]
    InvalidExtGStateEntryStructure {
        entry: String,
        expected_structure: &'static str,
        actual_structure: String,
    },
    #[error("invalid /ExtGState entry '/{entry}': {reason}")]
    InvalidExtGStateEntryValue { entry: String, reason: String },
    #[error("invalid /PaintType value: {value}")]
    InvalidPaintType { value: i32 },
    #[error("invalid /PatternType value: {value}")]
    InvalidPatternType { value: i32 },
    #[error("invalid /TilingType value: {value}")]
    InvalidTilingType { value: i32 },
    #[error("invalid /ShadingType value: {value}")]
    InvalidShadingType { value: i32 },
    #[error("unsupported XObject /Subtype: '{subtype}'")]
    UnsupportedXObjectSubtype { subtype: String },
    #[error("missing required dictionary entry '/{entry}'")]
    MissingRequiredEntry { entry: &'static str },
    /// The `/SMask` entry referenced a non-image XObject.
    ///
    /// Per the PDF specification, soft masks must always be Image XObjects
    /// with a `/ColorSpace` of `/DeviceGray`.
    #[error("invalid soft mask XObject: /SMask must reference an image XObject")]
    InvalidSoftMaskXObject,
    /// The image has zero-area dimensions (width or height is zero).
    #[error("invalid image dimensions: width={width}, height={height}")]
    InvalidImageDimensions { width: usize, height: usize },
    /// The bits per component value is not supported.
    ///
    /// Only 1-bit and 8-bit-per-component images are currently supported.
    #[error("unsupported image BitsPerComponent value: {bits_per_component} (supported: 1, 8)")]
    UnsupportedImageBitsPerComponent { bits_per_component: usize },
    /// The color space reported zero color components, which is invalid.
    #[error("invalid image color space: reported zero color components")]
    InvalidColorComponentCount,
    #[error("truncated image data: expected at least {expected_bytes} bytes, got {actual_bytes}")]
    TruncatedImageData {
        expected_bytes: usize,
        actual_bytes: usize,
    },
    #[error("{0}")]
    FunctionRead(#[from] FunctionReadError),
}

impl PdfPagesError {
    pub(crate) fn is_cyclic_dependency(&self) -> bool {
        matches!(self, Self::Object(ObjectError::CyclicDependency { .. }))
    }
}
