use crate::error::PdfCanvasError;

/// A generic interface for rendering text content.
///
/// This trait abstracts the specifics of how text is drawn, allowing different
/// font types (like Type 3 or TrueType) to be handled by a common rendering pipeline.
pub trait TextRenderer {
    /// Renders a sequence of character codes at the current text position.
    ///
    /// The implementation is responsible for interpreting the `text` bytes according
    /// to the current font's encoding, calculating glyph positions, and drawing
    /// the glyphs onto the canvas.
    fn render_text(&mut self, text: &[u8]) -> Result<(), PdfCanvasError>;
}
