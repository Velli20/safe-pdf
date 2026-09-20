//! Errors produced by PDF interpretation, recording, and backend drawing.
use pdf_color_space::error::ColorSpaceError;
use pdf_graphics::dash_pattern::DashPatternError;
use thiserror::Error;

use crate::tiling_shader::TilingShaderError;

/// Defines errors that can occur during PDF canvas operations.
#[derive(Debug, Error)]
pub enum PdfCanvasError {
    /// Path geometry or its source-to-device mapping is nonfinite.
    #[error(transparent)]
    Transform(#[from] pdf_graphics::transform::TransformError),
    /// Invalid page or bitmap coordinate mapping.
    #[error(transparent)]
    Viewport(#[from] crate::ViewportError),
    /// Invalid portable mask geometry.
    #[error(transparent)]
    Mask(#[from] crate::mask_layer::MaskError),
    /// Recording commands contain an invalid scope boundary.
    #[error(transparent)]
    Recording(#[from] crate::recording_canvas::RecordingError),
    /// Command storage could not be allocated.
    #[error(transparent)]
    Allocation(#[from] std::collections::TryReserveError),
    /// Both an operation and its explicit restoration failed.
    #[error("{primary}; restoration also failed: {cleanup}")]
    Cleanup {
        /// Original operation failure.
        #[source]
        primary: Box<PdfCanvasError>,
        /// Failure while restoring caller state.
        cleanup: Box<PdfCanvasError>,
    },
    /// A deferred resource could not be accessed.
    #[error(transparent)]
    ObjectRead(#[from] pdf_object_reader::ObjectReadError),
    #[error("The current operation requires an active path, but no path has been started")]
    /// The operator needs an active path.
    PathRequired,
    #[error("The current operation requires a current point, but no current point is set")]
    /// The path operator needs a current point.
    CurrentPointRequired,
    #[error("The current operation requires a current font, but no font is selected")]
    /// Text painting needs a selected font.
    CurrentFontRequired,
    #[error("Page resources are missing")]
    /// The page has no resource dictionary.
    PageResourcesMissing,
    #[error("Invalid font data: {0}")]
    /// Font data is invalid.
    InvalidFont(String),
    #[error(transparent)]
    /// Font loading or decoding failed.
    Font(#[from] pdf_font::FontError),
    #[error(transparent)]
    /// Text layout or glyph preparation failed.
    Text(#[from] pdf_text_engine::TextError),
    #[error("Font resource '{0}' was not found")]
    /// The named font is absent from resources.
    FontNotFound(String),
    #[error("Color space resource '{0}' was not found")]
    /// The named color space is absent from resources.
    ColorSpaceNotFound(String),
    #[error("Pattern resource '{0}' was not found")]
    /// The named pattern or shading is absent from resources.
    PatternNotFound(String),
    #[error("Graphics state stack is empty while accessing the current state")]
    /// No PDF graphics state is available.
    EmptyGraphicsStateStack,
    #[error("External object (XObject) '{0}' was not found in page resources")]
    /// The named external object is absent from resources.
    XObjectNotFound(String),
    #[error("Invalid image data: {0}")]
    /// Image data is invalid.
    InvalidImageData(String),
    #[error("Invalid dash pattern: {0}")]
    /// Dash intervals or their phase are invalid.
    InvalidDashPattern(String),
    #[error("The current operation requires a color space, but none is set")]
    /// Color operands require a selected color space.
    ColorSpaceNotSet,
    #[error("Unsupported PDF canvas feature: {0}")]
    /// The requested PDF feature is unsupported.
    UnsupportedFeature(String),
    #[error("Canvas backend error: {0}")]
    /// A concrete backend reported a rendering failure.
    BackendError(String),
    #[error("Color space error: {0}")]
    /// Color conversion failed.
    ColorSpaceError(#[from] ColorSpaceError),
    #[error(transparent)]
    /// Portable pixel processing of mask coverage failed.
    Raster(#[from] pdf_image::ImageRasterError),
}

impl From<DashPatternError> for PdfCanvasError {
    /// Converts this error at the canvas boundary while retaining its diagnostic.
    fn from(error: DashPatternError) -> Self {
        Self::InvalidDashPattern(error.to_string())
    }
}

impl From<TilingShaderError> for PdfCanvasError {
    /// Preserves the canvas error category and diagnostics for rejected tiling patterns.
    fn from(error: TilingShaderError) -> Self {
        Self::UnsupportedFeature(error.to_string())
    }
}
