use pdf_graphics::{
    BlendMode, LineCap, LineJoin, color::Color, pdf_path::PdfPath, transform::Transform,
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
    /// The current resource dictionary, overriding the page's resources if set.
    pub resources: Option<&'a Resources>,
    /// The current pattern (shading or tiling) used for filling or stroking.
    pub pattern: Option<&'a Pattern>,
    /// The current blend mode, controlling compositing behavior.
    pub blend_mode: Option<BlendMode>,
}

impl CanvasState<'_> {
    /// Default line width in user space units.
    const DEFAULT_LINE_WIDTH: f32 = 1.0;
    /// Default fill color.
    const DEFAULT_FILL_COLOR: Color = Color::from_rgb(0.0, 0.0, 0.0);
    /// Default stroke color.
    const DEFAULT_STROKE_COLOR: Color = Color::from_rgb(0.0, 0.0, 0.0);
    /// Default miter limit.
    const DEFAULT_MITER_LIMIT: f32 = 0.0;
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
            pattern: None,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            blend_mode: None,
        }
    }
}
