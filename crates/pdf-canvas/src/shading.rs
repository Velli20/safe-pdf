//! Shading operators and shared paint preparation.
use crate::CanvasPath;
use pdf_content_stream_operators::pdf_operator_backend::ShadingOps;
use pdf_graphics::{PathFillType, pdf_path::PdfPath, rect::Rect, transform::Transform};
use pdf_shading::model::Shading;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> ShadingOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    /// Prepares a shading paint and fills the current clip through the selected soft mask.
    fn paint_shading(&mut self, shading_name: &[u8]) -> Result<(), Self::ErrorType> {
        let state = self.current_state()?;

        let Some(shading) = state
            .resources
            .as_ref()
            .and_then(|r| r.shading(shading_name))
        else {
            return Err(PdfCanvasError::PatternNotFound(
                String::from_utf8_lossy(shading_name).into_owned(),
            ));
        };

        // Paints the area of the current clipping path with the shading pattern named
        let path = if let Some(clip) = &state.clip_path {
            clip.clone()
        } else {
            // If no clip path exists, the entire page is used.
            PdfPath::from(&Rect::new(self.canvas.width(), self.canvas.height()))
        };

        let fill_color = state.paint.fill_color;
        let blend_mode = state.paint.blend_mode.clone();
        let mat = state.transform;
        let bbox_clip = shading_bbox_clip(&shading, &mat);
        let shader = Some(self.build_shading_shader(&shading, &Some(mat), false)?);

        self.with_soft_mask(|backend| {
            with_clip(backend, bbox_clip.as_ref(), |backend| {
                backend.fill_path(
                    &CanvasPath::device(&path),
                    PathFillType::Winding,
                    fill_color,
                    shader.as_ref(),
                    blend_mode,
                )
            })
        })
    }
}

/// Returns the shading's `/BBox` mapped through `transform` into device space.
pub(crate) fn shading_bbox_clip(shading: &Shading, transform: &Transform) -> Option<PdfPath> {
    let mut clip = PdfPath::from(shading.bbox()?);
    clip.transform(transform);
    Some(clip)
}

/// Runs `paint` with the backend clipped to `clip`, restoring the clip afterwards.
pub(crate) fn with_clip<B: CanvasBackend + ?Sized>(
    backend: &mut B,
    clip: Option<&PdfPath>,
    paint: impl FnOnce(&mut B) -> Result<(), PdfCanvasError>,
) -> Result<(), PdfCanvasError> {
    let Some(clip) = clip else {
        return paint(backend);
    };
    backend.save()?;
    let result = backend
        .set_clip_region(&CanvasPath::device(clip), PathFillType::Winding)
        .and_then(|()| paint(backend));
    backend.restore()?;
    result
}
