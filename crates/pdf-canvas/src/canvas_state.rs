use pdf_color_space::color_space::ColorSpace;
use pdf_graphics::{
    BlendMode, DashPattern, LineCap, LineJoin, TextRenderingMode, color::Color, pdf_path::PdfPath,
    transform::Transform,
};
use pdf_page::{pattern::Pattern, resources::Resources};

use crate::text_state::TextState;

/// Represents the complete graphics state for a PDF canvas, including
/// transformation, color, stroke, text, and pattern information.
#[derive(Clone)]
pub(crate) struct CanvasState<'a> {
    /// The current transformation matrix, mapping user space to device space.
    pub transform: Transform,
    /// The current stroke color used for outlining paths.
    pub stroke_color: Color,
    /// The current fill color used for filling paths and text.
    pub fill_color: Color,
    /// The current line width for stroking paths, in user space units.
    pub line_width: f32,
    /// The current miter limit for joins, controlling how sharp corners are rendered.
    pub miter_limit: f32,
    /// The current text state, encapsulating font, size, and text matrix.
    pub text_state: TextState<'a>,
    /// The current clipping path, if any, restricting drawing to a region.
    pub clip_path: Option<PdfPath>,
    /// The current line cap style (butt, round, or projecting square).
    pub line_cap: LineCap,
    /// The current line join style (miter, round, or bevel).
    pub line_join: LineJoin,
    /// The current dash pattern for stroking paths.
    pub dash_pattern: Option<DashPattern>,
    /// The current resource dictionary, overriding the page's resources if set.
    pub resources: Option<&'a Resources>,
    /// The current pattern used for filling.
    pub fill_pattern: Option<&'a Pattern>,
    /// The current pattern used for stroking.
    pub stroke_pattern: Option<&'a Pattern>,
    /// The current blend mode, controlling compositing behavior.
    pub blend_mode: Option<BlendMode>,
    /// The current text rendering mode.
    pub rendering_mode: TextRenderingMode,
    /// Accumulated glyph outlines (in device space) for clip-mode text rendering (modes 4–7).
    ///
    /// Glyphs are appended here during a text object and the resulting path is applied
    /// as a clip region when the text object ends (ET operator).
    pub pending_text_clip: Option<PdfPath>,
    /// The current color space for stroking operations.
    pub stroke_color_space: Option<&'a ColorSpace>,
    /// The current color space for non-stroking operations.
    pub fill_color_space: Option<&'a ColorSpace>,
}

impl CanvasState<'_> {
    /// Default line width in user space units.
    const DEFAULT_LINE_WIDTH: f32 = 1.0;
    /// Default fill color.
    pub const DEFAULT_FILL_COLOR: Color = Color::from_rgb(0.0, 0.0, 0.0);
    /// Default stroke color.
    pub const DEFAULT_STROKE_COLOR: Color = Color::from_rgb(0.0, 0.0, 0.0);
    /// Default miter limit.
    const DEFAULT_MITER_LIMIT: f32 = 10.0;
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

impl Default for CanvasState<'_> {
    fn default() -> Self {
        Self {
            transform: Transform::identity(),
            stroke_color: Self::DEFAULT_STROKE_COLOR,
            fill_color: Self::DEFAULT_FILL_COLOR,
            line_width: Self::DEFAULT_LINE_WIDTH,
            miter_limit: Self::DEFAULT_MITER_LIMIT,
            text_state: TextState::default(),
            clip_path: None,
            resources: None,
            fill_pattern: None,
            stroke_pattern: None,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            dash_pattern: None,
            blend_mode: None,
            rendering_mode: TextRenderingMode::Fill,
            pending_text_clip: None,
            stroke_color_space: Some(&Self::DEFAULT_COLOR_SPACE),
            fill_color_space: Some(&Self::DEFAULT_COLOR_SPACE),
        }
    }
}
