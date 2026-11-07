//! Isolated group rendering: content is painted on a full backing-size surface, then
//! composited onto its parent as one object.

use crate::{
    error::WebResult, surface::Surface, web_canvas_backend::WebCanvasBackend,
    web_mask::composite_surface, web_paint,
};
use pdf_canvas::error::PdfCanvasError;
use pdf_graphics::BlendMode;

/// A group's isolated child canvas, owned as one rendering operation.
///
/// Construction installs inherited clips before the callback can run. Consuming
/// paint() composites only successful, balanced content, then resets and recycles
/// the surface even when the callback returns an error.
pub(crate) struct IsolatedGroup<'a> {
    parent: &'a WebCanvasBackend,
    backend: WebCanvasBackend,
    content: Surface,
}

impl<'a> IsolatedGroup<'a> {
    pub(crate) fn new(parent: &'a WebCanvasBackend) -> WebResult<Self> {
        let content = parent.surfaces.acquire(parent.viewport.backing_size())?;
        let backend = parent.isolated_on(&content)?;
        Ok(Self {
            parent,
            backend,
            content,
        })
    }

    /// Paints the group, then composites it with `alpha` and `blend_mode`.
    pub(crate) fn paint<F>(
        self,
        alpha: f32,
        blend_mode: Option<&BlendMode>,
        paint: F,
    ) -> Result<(), PdfCanvasError>
    where
        F: FnOnce(&mut WebCanvasBackend) -> Result<(), PdfCanvasError>,
    {
        let Self {
            parent,
            backend,
            content,
        } = self;
        let result = {
            let mut backend = backend;
            paint(&mut backend).and_then(|()| backend.finish_page().map_err(Into::into))
        };
        let result = result.and_then(|()| {
            composite_surface(
                &parent.context,
                &content,
                alpha,
                web_paint::blend(blend_mode),
            )
            .map_err(Into::into)
        });
        // The child backend is gone before the canvas is reset for reuse.
        parent.surfaces.recycle(content);
        result
    }
}
