use pdf_color_space::error::ColorSpaceError;
use pdf_content_stream_operators::error::PdfOperatorError;
use pdf_decode::DecodeError;
use pdf_font::error::FontError;
use pdf_function::{
    error::FunctionReadError, function_interpolation_error::FunctionInterpolationError,
};
use pdf_graphics::dash_pattern::DashPatternError;
use pdf_image::PdfImageError;
use pdf_shading::error::PdfShadingError;

use thiserror::Error;

/// Errors that can occur during parsing of a PDF Pages object.
#[derive(Error, Debug)]
pub enum PdfPagesError {
    /// Failure in a nested typed object read.
    #[error(transparent)]
    ObjectRead(#[from] pdf_object_reader::ObjectReadError),
    #[error("invalid /Kids entry type: expected /Page or /Pages, found '{found_type}'")]
    InvalidKidsEntryType { found_type: String },
    #[error("{0}")]
    Object(#[from] pdf_object_reader::object_error::ObjectError),
    #[error("failed to parse content stream: {0}")]
    ContentStream(#[from] PdfOperatorError),
    #[error("{0}")]
    ColorSpace(#[from] ColorSpaceError),
    #[error("{0}")]
    FunctionInterpolation(#[from] FunctionInterpolationError),
    #[error("failed to process font: {0}")]
    Font(#[from] FontError),
    #[error("invalid /ExtGState entry '/{entry}': expected an array of 2 elements, found {found}")]
    InvalidExtGStateArrayLength { entry: &'static str, found: usize },
    #[error("invalid /ExtGState entry '/D': {0}")]
    InvalidExtGStateDashPattern(#[from] DashPatternError),
    #[error("invalid /ExtGState entry '/LC': unsupported line cap value {0} (expected 0, 1, or 2)")]
    InvalidExtGStateLineCap(i32),
    #[error(
        "invalid /ExtGState entry '/LJ': unsupported line join value {0} (expected 0, 1, or 2)"
    )]
    InvalidExtGStateLineJoin(i32),
    #[error(
        "invalid /ExtGState entry '/SMask': expected a soft mask dictionary or the name 'None'"
    )]
    InvalidExtGStateSoftMask,
    #[error(
        "invalid /ExtGState entry '/SMask': group XObject must have /Subtype /Form, found /{subtype}"
    )]
    SoftMaskGroupNotForm { subtype: String },
    #[error("invalid /ExtGState entry '/SMask': /TR must produce one finite output")]
    InvalidSoftMaskTransfer,
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
    #[error("failed to process image: {0}")]
    Image(#[from] PdfImageError),
    #[error("{0}")]
    FunctionRead(#[from] FunctionReadError),
    #[error("{0}")]
    Decode(#[from] DecodeError),
    #[error("{0}")]
    Shading(#[from] PdfShadingError),
    #[error("invalid annotation entry '/{entry:?}': {reason}")]
    InvalidAnnotationEntry {
        entry: &'static [u8],
        reason: String,
    },
    #[error("missing required annotation entry '/{entry:?}'")]
    MissingAnnotationEntry { entry: &'static [u8] },
}

impl From<PdfPagesError> for pdf_object_reader::ObjectReadError {
    fn from(source: PdfPagesError) -> Self {
        match source {
            PdfPagesError::ObjectRead(error) => error,
            source => Self::Decode {
                target: "PDF resource",
                source: Box::new(source),
            },
        }
    }
}
