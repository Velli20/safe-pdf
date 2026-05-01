use pdf_color_space::error::ColorSpaceError;
use pdf_content_stream::error::PdfOperatorError;
use pdf_font::error::FontError;
use pdf_function::{
    error::FunctionReadError, function_interpolation_error::FunctionInterpolationError,
};
use pdf_image::PdfImageError;

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
    #[error("failed to process image: {0}")]
    Image(#[from] PdfImageError),
    #[error("{0}")]
    FunctionRead(#[from] FunctionReadError),
}

impl PdfPagesError {
    pub(crate) fn is_cyclic_dependency(&self) -> bool {
        matches!(self, Self::Object(ObjectError::CyclicDependency { .. }))
            || matches!(self, Self::Image(err) if err.is_cyclic_dependency())
    }
}
