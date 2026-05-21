use pdf_graphics::DashPattern;

/// Stroke-specific rendering metadata passed to canvas backends.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrokeStyle {
    /// Optional dash pattern. `None` means a solid stroke.
    pub dash_pattern: Option<DashPattern>,
}

impl StrokeStyle {
    /// Returns a stroke style scaled into the same coordinate space as the stroked path.
    pub fn scaled(&self, scale: f32) -> Self {
        Self {
            dash_pattern: self
                .dash_pattern
                .as_ref()
                .map(|dash_pattern| dash_pattern.scaled(scale)),
        }
    }
}
