use pdf_graphics::PathFillType;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Handles `W`: marks the current path as a nonzero-winding clip.
    pub(crate) fn clip_path_nonzero_winding(&mut self) -> Result<(), PdfCanvasError> {
        self.mark_clip_path(PathFillType::Winding)
    }

    /// Handles `W*`: marks the current path as an even-odd clip.
    pub(crate) fn clip_path_even_odd(&mut self) -> Result<(), PdfCanvasError> {
        self.mark_clip_path(PathFillType::EvenOdd)
    }
}

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Marks the path ended by the next path-painting operator for clipping with `rule`.
    ///
    /// The mark applies even when `W`/`W*` comes before the path is built. If that
    /// operator ends no path, the mark is dropped and the clipping path stays unchanged.
    fn mark_clip_path(&mut self, rule: PathFillType) -> Result<(), PdfCanvasError> {
        self.pending_clip = Some(rule);
        Ok(())
    }
}
