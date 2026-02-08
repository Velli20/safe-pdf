use pdf_content_stream::pdf_operator_backend::ShadingOps;
use pdf_graphics::pdf_path::PdfPath;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> ShadingOps for PdfCanvas<'_, B> {
    fn paint_shading(&mut self, shading_name: &str) -> Result<(), Self::ErrorType> {
        let state = self.current_state()?;

        let Some(shading) = state.resources.and_then(|r| r.shadings.get(shading_name)) else {
            return Err(PdfCanvasError::PatternNotFound(shading_name.to_string()));
        };

        // Paints the area of the current clipping path with the shading pattern named
        let path = if let Some(clip) = &state.clip_path {
            clip.clone()
        } else {
            // If no clip path exists, the entire page is used.
            let mut path = PdfPath::default();

            path.move_to(0.0, 0.0);
            path.line_to(self.canvas.width(), 0.0);
            path.line_to(self.canvas.width(), self.canvas.height());
            path.line_to(0.0, self.canvas.height());
            path.line_to(0.0, 0.0);
            path.close();
            path
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

        self.restore()
    }
}
