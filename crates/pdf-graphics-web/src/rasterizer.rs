//! Budgeted rasterization without page or annotation semantics.

use crate::{BudgetedImage, WebCanvasBackend, error::WebResult, surface_pool::SurfacePool};
use pdf_canvas::{
    CanvasViewport, ViewportError, canvas_backend::CanvasBackend, recording_canvas::RecordingCanvas,
};

/// Owns reusable Canvas 2D surfaces and accounting for retained raster images.
pub struct WebRasterizer {
    surfaces: SurfacePool,
}

impl WebRasterizer {
    /// Creates a rasterizer with a byte budget shared by surfaces and image readbacks.
    pub fn new(temporary_bytes: usize) -> Self {
        Self {
            surfaces: SurfacePool::new(temporary_bytes),
        }
    }

    /// Replays device-space drawing into the supplied bitmap viewport and reads its pixels.
    /// Returned image clones share their storage reservation until the last clone is dropped.
    pub fn rasterize(
        &self,
        recording: &RecordingCanvas,
        viewport: &CanvasViewport,
    ) -> WebResult<BudgetedImage> {
        if viewport.device_size() != [recording.width(), recording.height()] {
            return Err(ViewportError::DeviceSizeMismatch.into());
        }
        let surface = self.surfaces.acquire(viewport.backing_size())?;
        let mut backend = WebCanvasBackend::on_surface(
            &surface,
            viewport.device_size(),
            *viewport.device_to_backing(),
            self.surfaces.clone(),
        )?;
        backend.replay_balanced(recording)?;
        drop(backend);
        let image = self.surfaces.read(&surface)?;
        self.surfaces.recycle(surface);
        Ok(image)
    }

    /// Releases cached surfaces; retained images keep their reservations.
    pub fn clear(&self) {
        self.surfaces.clear();
    }
}
