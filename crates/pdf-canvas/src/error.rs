use thiserror::Error;

use crate::truetype_font_renderer::TrueTypeFontRendererError;
use crate::type3_font_renderer::Type3FontRendererError;

/// Defines errors that can occur during PDF canvas operations.
#[derive(Debug, Error)]
pub enum PdfCanvasError {
    #[error("No active path to perform the painting operation")]
    NoActivePath,
    #[error("Operation requires a current point, but none is set")]
    NoCurrentPoint,
    #[error("Operation requires a current font, but none is set")]
    NoCurrentFont,
    #[error("Missing page resources")]
    MissingPageResources,
    #[error("Invalid font: {0}")]
    InvalidFont(&'static str),
    #[error("Font '{0}' not found")]
    FontNotFound(String),
    #[error("Graphics state dictionary '{0}' not found in resources")]
    GraphicsStateNotFound(String),
    #[error("Font '{0}' is a Type3 font but is missing its definition data")]
    MissingType3FontData(String),
    #[error(
        "Graphics state stack is empty, cannot access current state. This indicates an internal error."
    )]
    EmptyGraphicsStateStack,
    #[error("Cannot restore graphics state: stack underflow (no state to restore).")]
    GraphicsStateStackUnderflow,
    #[error("TrueType font rendering error: {0}")]
    TrueTypeFontError(#[from] TrueTypeFontRendererError),
    #[error("Type3 font rendering error: {0}")]
    Type3FontError(#[from] Type3FontRendererError),
    #[error("Extrenal object '{0}' not found in resources")]
    XObjectNotFound(String),
    #[error("Page missing media box")]
    MissingMediaBox,
}
