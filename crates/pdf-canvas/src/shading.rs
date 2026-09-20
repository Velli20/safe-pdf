//! Shading operators and shared paint preparation.
use crate::CanvasPath;
use pdf_content_stream_operators::pdf_operator_backend::ShadingOps;
use pdf_graphics::{pdf_path::PdfPath, rect::Rect};

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
        let shader = Some(self.build_shading_shader(&shading, &Some(mat))?);

        self.with_soft_mask(|backend| {
            backend.fill_path(
                &CanvasPath::device(&path),
                pdf_graphics::PathFillType::Winding,
                fill_color,
                shader.as_ref(),
                blend_mode,
            )
        })
    }
}
