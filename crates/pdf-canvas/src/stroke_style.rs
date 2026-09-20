use pdf_graphics::{CanvasPaint, DashPattern, LineCap, LineJoin, dash_pattern::DashPatternError};

/// Stroke-specific rendering metadata passed to canvas backends.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    /// Optional dash pattern. `None` means a solid stroke.
    pub dash_pattern: Option<DashPattern>,
    /// Shape of open stroke endpoints.
    pub line_cap: LineCap,
    /// Shape of stroke joins.
    pub line_join: LineJoin,
    /// Dimensionless miter limit.
    pub miter_limit: f32,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        let paint = CanvasPaint::default();
        Self {
            dash_pattern: None,
            line_cap: paint.line_cap,
            line_join: paint.line_join,
            miter_limit: paint.miter_limit,
        }
    }
}

impl StrokeStyle {
    /// Converts paint metadata to device space without an intermediate dash clone.
    pub fn from_paint(paint: &CanvasPaint, scale: f32) -> Result<Self, DashPatternError> {
        Ok(Self {
            dash_pattern: paint
                .dash_pattern
                .as_ref()
                .map(|pattern| pattern.scaled(scale))
                .transpose()?,
            line_cap: paint.line_cap,
            line_join: paint.line_join,
            miter_limit: paint.miter_limit,
        })
    }

    /// Returns a stroke style scaled into the same coordinate space as the stroked path.
    pub fn scaled(&self, scale: f32) -> Result<Self, DashPatternError> {
        Ok(Self {
            dash_pattern: self
                .dash_pattern
                .as_ref()
                .map(|dash_pattern| dash_pattern.scaled(scale))
                .transpose()?,
            line_cap: self.line_cap,
            line_join: self.line_join,
            miter_limit: self.miter_limit,
        })
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
