//! Browser-specific failures translated into the existing shared canvas error.

use pdf_canvas::error::PdfCanvasError;

/// Failure while preparing browser rendering resources or drawing.
#[derive(Debug, thiserror::Error)]
pub enum WebCanvasBackendError {
    /// The host canvas cannot provide a two-dimensional rendering context.
    #[error("Canvas 2D context unavailable")]
    ContextUnavailable,
    /// A caught browser exception, preserved until conversion into the shared error.
    #[error("browser operation failed: {0:?}")]
    Browser(wasm_bindgen::JsValue),
    /// Nonfinite geometry, invalid dimensions, or invalid image/shader input.
    #[error("invalid web rendering input: {0}")]
    InvalidInput(&'static str),
    /// Resource allocation would exceed the configured memory budget.
    #[error("web rendering resource budget exceeded")]
    ResourceLimit,
    /// Restore or mask completion does not match the current runtime state stack.
    #[error("unbalanced web graphics state")]
    UnbalancedState,
    /// An upstream PDF operation failed while preparing content or an appearance.
    #[error(transparent)]
    Canvas(#[from] PdfCanvasError),
    /// A bitmap viewport has invalid geometry.
    #[error(transparent)]
    Viewport(#[from] pdf_canvas::ViewportError),
}

impl From<WebCanvasBackendError> for PdfCanvasError {
    /// Uses the existing BackendError string boundary, preserving Canvas variants directly.
    /// Structured JavaScript exception information cannot survive the string conversion.
    fn from(error: WebCanvasBackendError) -> Self {
        match error {
            WebCanvasBackendError::Canvas(error) => error,
            other => Self::BackendError(other.to_string()),
        }
    }
}

/// Result for web-specific APIs; the existing CanvasBackend methods use PdfCanvasError.
pub type WebResult<T> = Result<T, WebCanvasBackendError>;

impl From<pdf_graphics::transform::TransformError> for WebCanvasBackendError {
    fn from(error: pdf_graphics::transform::TransformError) -> Self {
        match error {
            pdf_graphics::transform::TransformError::NonFinite => {
                Self::InvalidInput("nonfinite transform")
            }
            pdf_graphics::transform::TransformError::Singular => {
                Self::InvalidInput("singular transform")
            }
        }
    }
}

impl From<wasm_bindgen::JsValue> for WebCanvasBackendError {
    fn from(value: wasm_bindgen::JsValue) -> Self {
        Self::Browser(value)
    }
}

impl From<pdf_shading::error::ShadingRasterError> for WebCanvasBackendError {
    fn from(error: pdf_shading::error::ShadingRasterError) -> Self {
        match error {
            pdf_shading::error::ShadingRasterError::InvalidInput(message) => {
                Self::InvalidInput(message)
            }
            pdf_shading::error::ShadingRasterError::ResourceLimit => Self::ResourceLimit,
        }
    }
}

impl From<pdf_image::ImageRasterError> for WebCanvasBackendError {
    /// Preserves raster validation and allocation error categories at the web boundary.
    fn from(error: pdf_image::ImageRasterError) -> Self {
        match error {
            pdf_image::ImageRasterError::InvalidInput(message) => Self::InvalidInput(message),
            pdf_image::ImageRasterError::ResourceLimit => Self::ResourceLimit,
        }
    }
}
