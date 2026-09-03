//! PDF text-state values that affect text layout.

/// Style and PDF text-state values that affect layout.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// Font size in text-space units.
    pub font_size: f32,
    /// Additional spacing applied after every character.
    pub character_spacing: f32,
    /// Additional spacing applied to PDF word-space characters.
    pub word_spacing: f32,
    /// Horizontal scale where `1.0` represents 100 percent.
    pub horizontal_scale: f32,
    /// Baseline rise in text-space units.
    pub rise: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 0.0,
            character_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scale: 1.0,
            rise: 0.0,
        }
    }
}
