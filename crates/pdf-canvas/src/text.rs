use pdf_cmap::UnicodeSequence;
use pdf_graphics::rect::Rect;

/// One selectable glyph span in rendered device coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct TextGlyph {
    /// Unicode scalar values represented by this glyph span.
    pub unicode: UnicodeSequence,
    /// Axis-aligned device-space bounds for hit testing and highlighting.
    pub bounds: Rect,
}
