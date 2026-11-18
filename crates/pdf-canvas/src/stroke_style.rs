use std::sync::Arc;

use pdf_graphics::{
    CanvasPaint, DashPattern, LineCap, LineJoin,
    transform::{Transform, TransformError},
};

use crate::CanvasPath;

/// Stroke-specific rendering metadata passed to canvas backends.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    /// Optional dash pattern in stroke space. `None` means a solid stroke.
    ///
    /// Shared so recording a stroke does not copy the intervals.
    pub dash_pattern: Option<Arc<DashPattern>>,
    /// Shape of open stroke endpoints.
    pub line_cap: LineCap,
    /// Shape of stroke joins.
    pub line_join: LineJoin,
    /// Dimensionless miter limit.
    pub miter_limit: f32,
    /// Linear map from stroke space, where the line width and dash lengths are measured,
    /// to logical device space. Its determinant is ±1, so it is a rotation or reflection
    /// unless the CTM scales unevenly or shears.
    pub transform: Transform,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        let paint = CanvasPaint::default();
        Self {
            dash_pattern: None,
            line_cap: paint.line_cap,
            line_join: paint.line_join,
            miter_limit: paint.miter_limit,
            transform: Transform::identity(),
        }
    }
}

impl StrokeStyle {
    /// Converts paint metadata to the stroke space of `ctm`.
    ///
    /// `dash_pattern` is the paint's dash already scaled by [`Transform::area_scale`], so
    /// strokes sharing a dash and scale share one pattern. The remaining linear part of `ctm`
    /// becomes [`StrokeStyle::transform`]. A singular `ctm` keeps an identity transform.
    pub fn new(
        paint: &CanvasPaint,
        ctm: &Transform,
        dash_pattern: Option<Arc<DashPattern>>,
    ) -> Self {
        let scale = ctm.area_scale();
        let transform = if scale > 0.0 && scale.is_finite() {
            let mut linear = ctm.linear();
            linear.scale(scale.recip(), scale.recip());
            linear
        } else {
            Transform::identity()
        };
        Self {
            dash_pattern,
            line_cap: paint.line_cap,
            line_join: paint.line_join,
            miter_limit: paint.miter_limit,
            transform,
        }
    }

    /// Returns `path` mapped into stroke space when stroking its device geometry would
    /// distort the pen, or `None` when [`StrokeStyle::transform`] preserves angles or
    /// `line_width` resolves to a device hairline.
    ///
    /// A backend strokes the returned geometry with the line width and dash lengths, under
    /// [`StrokeStyle::transform`], so that the outline matches a stroke in user space.
    pub fn stroke_space_path<'p>(
        &self,
        path: &'p CanvasPath<'_>,
        line_width: f32,
    ) -> Result<Option<CanvasPath<'p>>, TransformError> {
        if line_width <= 0.0 || self.transform.is_similarity() {
            return Ok(None);
        }
        let inverse = self.transform.try_inverse()?;
        path.remapped(&inverse).map(Some)
    }
}

/// Resolves a device-space stroke width for a backend.
///
/// PDF renders nonpositive widths as the thinnest line the device can draw, and
/// reflected CTMs can yield negative widths; both resolve to `hairline`. Nonfinite
/// widths yield `None`.
pub fn device_stroke_width(width: f32, hairline: f32) -> Option<f32> {
    if !width.is_finite() {
        return None;
    }
    Some(if width <= 0.0 { hairline } else { width })
}
