use pdf_color_space::color_space::ColorSpace;
use pdf_graphics::{CanvasPaint, pdf_path::PdfPath, transform::Transform};
use pdf_resources::{pattern::Pattern, resources::Resources};
use std::sync::Arc;

use crate::text_state::TextState;

/// Represents the complete graphics state for a PDF canvas, including
/// transformation, color, stroke, text, and pattern information.
#[derive(Clone)]
pub(crate) struct CanvasState {
    /// The current transformation matrix, mapping user space to device space.
    pub transform: Transform,
    /// Paint properties used for paths and text glyphs.
    pub paint: CanvasPaint,
    /// The current text state, encapsulating font, size, and text matrix.
    pub text_state: TextState,
    /// The current clipping path, if any, restricting drawing to a region.
    pub clip_path: Option<PdfPath>,
    /// The current resource dictionary, overriding the page's resources if set.
    pub resources: Option<Arc<Resources>>,
    /// The current pattern used for filling.
    pub fill_pattern: Option<Arc<Pattern>>,
    /// The current pattern used for stroking.
    pub stroke_pattern: Option<Arc<Pattern>>,
    /// Accumulated glyph outlines (in device space) for clip-mode text rendering (modes 4–7).
    ///
    /// Glyphs are appended here during a text object and the resulting path is applied
    /// as a clip region when the text object ends (ET operator).
    pub pending_text_clip: Option<PdfPath>,
    /// The current color space for stroking operations.
    pub stroke_color_space: Option<Arc<ColorSpace>>,
    /// The current color space for non-stroking operations.
    pub fill_color_space: Option<Arc<ColorSpace>>,
}

impl CanvasState {
    /// Default color space for both stroking and non-stroking operations.
    const DEFAULT_COLOR_SPACE: ColorSpace = ColorSpace::DeviceGray;
    /// Static DeviceGray color space.
    pub const DEVICE_GRAY_COLOR_SPACE: ColorSpace = ColorSpace::DeviceGray;
    /// Static DeviceRGB color space.
    pub const DEVICE_RGB_COLOR_SPACE: ColorSpace = ColorSpace::DeviceRGB;
    /// Static DeviceCMYK color space.
    pub const DEVICE_CMYK_COLOR_SPACE: ColorSpace = ColorSpace::DeviceCMYK;
    /// Static bare Pattern color space (no underlying space).
    pub const PATTERN_COLOR_SPACE: ColorSpace = ColorSpace::Pattern(None);
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            transform: Transform::identity(),
            paint: CanvasPaint::default(),
            text_state: TextState::default(),
            clip_path: None,
            resources: None,
            fill_pattern: None,
            stroke_pattern: None,
            pending_text_clip: None,
            stroke_color_space: Some(Arc::new(Self::DEFAULT_COLOR_SPACE)),
            fill_color_space: Some(Arc::new(Self::DEFAULT_COLOR_SPACE)),
        }
    }
}
