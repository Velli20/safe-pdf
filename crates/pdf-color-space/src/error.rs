use thiserror::Error;

/// Errors that can occur when parsing PDF color spaces.
#[derive(Debug, Error)]
pub enum ColorSpaceError {
    #[error("Failed to resolve PDF object: {0}")]
    ObjectError(#[from] pdf_object_reader::object_error::ObjectError),
    #[error("Invalid or unsupported ColorSpace: {description}")]
    InvalidColorSpace { description: String },
    #[error("Function parsing error: {0}")]
    FunctionError(#[from] pdf_function::error::FunctionReadError),
    #[error("Indexed color space error: {0}")]
    IndexedColorSpaceError(String),
    #[error("Insufficient color components: expected {0}, found {1}")]
    InsufficientComponents(usize, usize),
    #[error("Unsupported color space: {0}")]
    Unsupported(String),
}

impl From<ColorSpaceError> for pdf_object_reader::ObjectReadError {
    fn from(source: ColorSpaceError) -> Self {
        Self::Decode {
            target: "PDF color space",
            source: Box::new(source),
        }
    }
}
