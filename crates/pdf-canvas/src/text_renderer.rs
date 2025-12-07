use crate::error::PdfCanvasError;

/// A generic interface for rendering text content.
///
/// This trait abstracts the specifics of how text is drawn, allowing different
/// font types (like Type 3 or TrueType) to be handled by a common rendering pipeline.
pub trait TextRenderer {
    /// Render a sequence of character codes at the current text position.
    ///
    /// # Parameters
    ///
    /// - `iter`: Iterator that yields `u16` character codes. Implementations may
    ///   treat these as single-byte codes (for Type1/TrueType) or two-byte
    ///   CIDs (for Type0/CID-keyed fonts), depending on the font in use.
    ///
    /// # Returns
    ///
    /// Returns [`PdfCanvasError`] when glyph lookup, font data, or drawing operations fail.
    fn render_text(&mut self, iter: &mut dyn Iterator<Item = u16>) -> Result<(), PdfCanvasError>;
}
