//! Shading operators and shared paint preparation.
use crate::CanvasPath;
use std::sync::Arc;

use pdf_graphics::{
    PathFillType,
    pdf_path::PdfPath,
    transform::{Transform, TransformError},
};
use pdf_shading::model::Shading;

use crate::{
    canvas_backend::{CanvasBackend, Shader},
    error::PdfCanvasError,
    pdf_canvas::PdfCanvas,
};

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Prepares a shading paint and fills the current clip through the selected soft mask.
    ///
    /// A shading name missing from the resources paints nothing.
    pub(crate) fn paint_shading(&mut self, shading_name: &[u8]) -> Result<(), PdfCanvasError> {
        let state = &self.state;

        let Some(shading) = state
            .resources
            .as_ref()
            .and_then(|r| r.shading(shading_name))
        else {
            // An unresolvable shading is skipped so the rest of the content still renders.
            return Ok(());
        };

        // Paints the area of the current clipping path with the shading pattern named
        let page;
        let path = match &state.clip_path {
            Some(clip) => clip.clone(),
            // If no clip path exists, the entire page is used.
            None => {
                page = PdfPath::from(&self.device_rect());
                CanvasPath::device(&page)
            }
        };

        let fill_color = state.paint.fill_color;
        let blend_mode = state.paint.blend_mode;
        let mat = state.transform;
        let bbox_clip = shading_bbox_clip(&shading, &mat)?;
        let shader = Some(Shader::Shading(shading.paint(&mat, false)?));

        self.with_soft_mask(|backend| {
            with_clip(backend, bbox_clip.as_ref(), |backend| {
                backend.fill_path(
                    &path,
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
pub(crate) fn shading_bbox_clip(
    shading: &Shading,
    transform: &Transform,
) -> Result<Option<CanvasPath<'static>>, TransformError> {
    shading
        .bbox()
        .map(|bbox| CanvasPath::shared(Arc::new(PdfPath::from(bbox)), *transform))
        .transpose()
}

/// Runs `paint` with the backend clipped to `clip`, restoring the clip afterwards.
pub(crate) fn with_clip<B: CanvasBackend + ?Sized>(
    backend: &mut B,
    clip: Option<&CanvasPath<'_>>,
    paint: impl FnOnce(&mut B) -> Result<(), PdfCanvasError>,
) -> Result<(), PdfCanvasError> {
    let Some(clip) = clip else {
        return paint(backend);
    };
    backend.save()?;
    let result = backend
        .set_clip_region(clip, PathFillType::Winding)
        .and_then(|()| paint(backend));
    backend.restore()?;
    result
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pdf_document::page::PdfPage;
    use pdf_resources::resources::Resources;
    use pdf_text_engine::bundled_font_system;

    use crate::{pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};

    #[test]
    /// Verifies that `sh` with a shading name missing from the resources paints nothing.
    fn missing_shading_is_skipped() {
        let page = PdfPage {
            contents: None,
            annotations: None,
            media_box: None,
            crop_box: None,
            resources: Some(Arc::new(Resources::default())),
            rotation: None,
            annotation_id_high_watermark: 0,
        };
        let mut backend = RecordingCanvas::new(100.0, 100.0);
        let mut canvas = PdfCanvas::new(&mut backend, &page, None, bundled_font_system())
            .expect("canvas should build");

        canvas
            .paint_shading(b"sh6")
            .expect("missing shading should be skipped");
    }
}
