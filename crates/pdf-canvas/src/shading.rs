use pdf_content_stream_operators::pdf_operator_backend::ShadingOps;
use pdf_graphics::{pdf_path::PdfPath, rect::Rect};

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> ShadingOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn paint_shading(&mut self, shading_name: &str) -> Result<(), Self::ErrorType> {
        let state = self.current_state()?;

        let Some(shading) = state.resources.and_then(|r| r.shading(shading_name)) else {
            return Err(PdfCanvasError::PatternNotFound(shading_name.to_string()));
        };

        // Paints the area of the current clipping path with the shading pattern named
        let path = if let Some(clip) = &state.clip_path {
            clip.clone()
        } else {
            // If no clip path exists, the entire page is used.
            PdfPath::from(&Rect::new(self.canvas.width(), self.canvas.height()))
        };

        let fill_color = state.fill_color;
        let blend_mode = state.blend_mode;
        let mat = state.transform;
        let shader = Some(self.build_shading_shader(shading, &Some(mat))?);

        self.save()?;
        self.canvas.fill_path(
            &path,
            pdf_graphics::PathFillType::Winding,
            fill_color,
            &shader,
            blend_mode,
        )?;

        self.restore();
        Ok(())
    }
}
