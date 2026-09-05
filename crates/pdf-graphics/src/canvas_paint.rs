//! Paint properties shared by canvas and text rendering operations.

use crate::{BlendMode, DashPattern, LineCap, LineJoin, TextRenderingMode, color::Color};

// The PDF specification's initial graphics state sets both stroking and non-stroking colors to
// black.
const DEFAULT_COLOR: Color = Color::from_rgb(0.0, 0.0, 0.0);
// The PDF specification's initial graphics state defines a line width of one user-space unit.
const DEFAULT_LINE_WIDTH: f32 = 1.0;
// The PDF specification's initial graphics state defines a miter limit of ten.
const DEFAULT_MITER_LIMIT: f32 = 10.0;

/// Paint properties used for paths and text glyphs.
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasPaint {
    /// Color used for stroking paths and glyph outlines.
    pub stroke_color: Color,
    /// Color used for filling paths and glyph interiors.
    pub fill_color: Color,
    /// Line width in user-space units.
    pub line_width: f32,
    /// Miter limit used for stroked joins.
    pub miter_limit: f32,
    /// Shape used at the ends of open stroked subpaths.
    pub line_cap: LineCap,
    /// Shape used at stroked path joins.
    pub line_join: LineJoin,
    /// Optional dash pattern. `None` represents a solid stroke.
    pub dash_pattern: Option<DashPattern>,
    /// Optional compositing blend mode.
    pub blend_mode: Option<BlendMode>,
    /// PDF text painting and clipping mode.
    pub rendering_mode: TextRenderingMode,
}

impl Default for CanvasPaint {
    fn default() -> Self {
        Self {
            stroke_color: DEFAULT_COLOR,
            fill_color: DEFAULT_COLOR,
            line_width: DEFAULT_LINE_WIDTH,
            miter_limit: DEFAULT_MITER_LIMIT,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            dash_pattern: None,
            blend_mode: None,
            rendering_mode: TextRenderingMode::Fill,
        }
    }
}
