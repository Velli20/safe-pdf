use pdf_color_space::error::ColorSpaceError;
use thiserror::Error;

/// Defines errors that can occur during PDF canvas operations.
#[derive(Debug, Error)]
pub enum PdfCanvasError {
    #[error("The current operation requires an active path, but no path has been started")]
    PathRequired,
    #[error("The current operation requires a current point, but no current point is set")]
    CurrentPointRequired,
    #[error("The current operation requires a current font, but no font is selected")]
    CurrentFontRequired,
    #[error("Page resources are missing")]
    PageResourcesMissing,
    #[error("Invalid font data: {0}")]
    InvalidFont(String),
    #[error("Font resource '{0}' was not found")]
    FontNotFound(String),
    #[error("Color space resource '{0}' was not found")]
    ColorSpaceNotFound(String),
    #[error("Pattern resource '{0}' was not found")]
    PatternNotFound(String),
    #[error("Graphics state stack is empty while accessing the current state")]
    EmptyGraphicsStateStack,
    #[error("Cannot restore graphics state because the stack is already at its base state")]
    GraphicsStateStackUnderflow,
    #[error("Failed to parse TrueType font data: {0}")]
    TrueTypeFontParse(String),
    #[error("External object (XObject) '{0}' was not found in page resources")]
    XObjectNotFound(String),
    #[error("Invalid image data: {0}")]
    InvalidImageData(String),
    #[error("Invalid dash pattern: {0}")]
    InvalidDashPattern(String),
    #[error("The current operation requires a color space, but none is set")]
    ColorSpaceNotSet,
    #[error("Unsupported PDF canvas feature: {0}")]
    UnsupportedFeature(String),
    #[error("Canvas backend error: {0}")]
    BackendError(String),
    #[error("Color space error: {0}")]
    ColorSpaceError(#[from] ColorSpaceError),
}
