use crate::PdfOperator;

/// Sets the character spacing, `Tc`, which is a number expressed in unscaled text space units. (PDF operator `Tc`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetCharacterSpacing {
    /// The character spacing. Added to the horizontal displacement otherwise produced by showing a glyph.
    spacing: f32,
}

impl PdfOperator for SetCharacterSpacing {
    fn operator() -> &'static str {
        "Tc"
    }
}
impl SetCharacterSpacing {
    pub fn new(spacing: f32) -> Self {
        Self { spacing }
    }
}

/// Sets the word spacing, `Tw`, which is a number expressed in unscaled text space units. (PDF operator `Tw`)
/// Word spacing is used by the `Tj`, `'`, and `"` operators.
#[derive(Debug, Clone, PartialEq)]
pub struct SetWordSpacing {
    /// The word spacing. Added to the character spacing when the character is a space (char code 32).
    spacing: f32,
}

impl PdfOperator for SetWordSpacing {
    fn operator() -> &'static str {
        "Tw"
    }
}

impl SetWordSpacing {
    pub fn new(spacing: f32) -> Self {
        Self { spacing }
    }
}

/// Sets the horizontal scaling, `Tz`, which adjusts the width of glyphs by stretching or compressing them horizontally. (PDF operator `Tz`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetHorizontalScaling {
    /// The horizontal scaling factor as a percentage (e.g., 100.0 for 100% - no scaling).
    scale: f32,
}

impl PdfOperator for SetHorizontalScaling {
    fn operator() -> &'static str {
        "Tz"
    }
}

impl SetHorizontalScaling {
    pub fn new(scale: f32) -> Self {
        Self { scale }
    }
}

/// Sets the text leading, `TL`, which is the vertical distance between the baselines of adjacent lines of text. (PDF operator `TL`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetLeading {
    /// The text leading, in unscaled text space units.
    leading: f32,
}

impl PdfOperator for SetLeading {
    fn operator() -> &'static str {
        "TL"
    }
}

impl SetLeading {
    pub fn new(leading: f32) -> Self {
        Self { leading }
    }
}

/// Sets the text font, `Tf`, to a font resource in the resource dictionary and the text font size, `Tfs`, in unscaled text space units. (PDF operator `Tf`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetFont {
    /// The name of the font resource.
    name: String,
    /// The font size.
    size: f32,
}

impl PdfOperator for SetFont {
    fn operator() -> &'static str {
        "Tf"
    }
}

impl SetFont {
    pub fn new(name: String, size: f32) -> Self {
        Self { name, size }
    }
}

/// Sets the text rendering mode, `Tr`, which determines whether text is filled, stroked, used as a clipping path, or some combination. (PDF operator `Tr`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetRenderingMode {
    /// The rendering mode.
    /// 0: Fill text.
    /// 1: Stroke text.
    /// 2: Fill, then stroke text.
    /// 3: Neither fill nor stroke text (invisible).
    /// 4: Fill text and add to path for clipping.
    /// 5: Stroke text and add to path for clipping.
    /// 6: Fill, then stroke text and add to path for clipping.
    /// 7: Add text to path for clipping.
    mode: u8,
}

impl PdfOperator for SetRenderingMode {
    fn operator() -> &'static str {
        "Tr"
    }
}

impl SetRenderingMode {
    pub fn new(mode: u8) -> Self {
        Self { mode }
    }
}

/// Sets the text rise, `Ts`, which specifies the vertical distance to shift the baseline of text relative to the current baseline. (PDF operator `Ts`)
#[derive(Debug, Clone, PartialEq)]
pub struct SetTextRise {
    /// The text rise, in unscaled text space units. A positive value moves the baseline up.
    rise: f32,
}

impl PdfOperator for SetTextRise {
    fn operator() -> &'static str {
        "Ts"
    }
}

impl SetTextRise {
    pub fn new(rise: f32) -> Self {
        Self { rise }
    }
}
