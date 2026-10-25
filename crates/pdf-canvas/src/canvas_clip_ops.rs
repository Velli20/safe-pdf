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
    /// Marks the current path for clipping with `rule`; the clip is applied when the next
    /// path-painting operator ends the path.
    fn mark_clip_path(&mut self, rule: PathFillType) -> Result<(), PdfCanvasError> {
        if self.current_path.is_none() {
            return Err(PdfCanvasError::PathRequired);
        }
        self.pending_clip = Some(rule);
        Ok(())
    }
}
