//! Soft-mask recording for an `ExtGState` `/SMask`.
use std::sync::Arc;

use pdf_graphics::{
    MaskMode, PaintMode, PathFillType, color::Color, size::Size, transform::Transform,
};
use pdf_resources::soft_mask::SoftMask;

use crate::{
    canvas_backend::CanvasBackend, canvas_state::CanvasState, error::PdfCanvasError,
    mask_layer::MaskLayer, pdf_canvas::PdfCanvas, recording_canvas::fits_offscreen_recording,
};

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Records the mask that `smask` selects, mapped from mask space to the device.
    ///
    /// A luminosity mask's group composites over its `/BC` backdrop, which also gives the
    /// mask its value outside the group's bounds. Such a mask is therefore recorded over
    /// the whole visible area: a backdrop fill, then the group clipped to its bounds.
    ///
    /// Returns `None` for an unknown mask subtype, or for a group without bounds or with
    /// bounds too large to record offscreen.
    pub(crate) fn soft_mask_layer(
        &mut self,
        smask: &SoftMask,
    ) -> Result<Option<MaskLayer>, PdfCanvasError> {
        if matches!(smask.mask_type, MaskMode::Unknown(_)) {
            return Ok(None);
        }
        let form = smask.shape.get()?;
        let Some(bbox) = form.bbox.as_ref() else {
            return Ok(None);
        };
        let matrix = form.matrix.unwrap_or_else(Transform::identity);
        let bounds = matrix.map_rect(bbox);
        if !fits_offscreen_recording(bbox) || !fits_offscreen_recording(&bounds) {
            return Ok(None);
        }
        let backdrop = match smask.mask_type {
            MaskMode::Luminosity => smask.backdrop,
            _ => None,
        };
        // Without a backdrop, coverage outside the group's bounds is zero, so the
        // recording needs to cover only those bounds.
        let region = match backdrop {
            // The visible area, mapped into the current user space.
            Some(_) => self
                .state
                .transform
                .inverse_map_rect(&self.device_rect())
                .ok()
                .filter(fits_offscreen_recording)
                .unwrap_or(bounds),
            None => bounds,
        };
        let size = region.size();
        // Backends cover only `[0, w] x [0, h]` of mask space, so mask space starts at
        // the region's origin in the current user space.
        let mask_matrix =
            Transform::from_translate(-region.left, -region.top).post_concatenated(&matrix);
        let recording = self.record_nested(size, CanvasState::default(), None, |canvas| {
            if let Some(color) = backdrop {
                canvas.paint_backdrop(color, size)?;
            }
            canvas.render_form(&form, Some(mask_matrix))
        })?;
        let transform = self
            .state
            .transform
            .post_concatenated(&Transform::from_translate(region.left, region.top));
        Ok(Some(MaskLayer::new(
            Arc::new(recording),
            transform,
            smask.mask_type.clone(),
            smask.transfer.clone(),
        )?))
    }

    /// Fills `[0, width] x [0, height]` with an opaque backdrop `color`, leaving the
    /// graphics state unchanged.
    fn paint_backdrop(&mut self, color: Color, size: Size<f32>) -> Result<(), PdfCanvasError> {
        self.save()?;
        self.state.paint.set_fill_color(color);
        self.current_path.rect(0.0, 0.0, size.width, size.height);
        self.end_path(Some((PaintMode::Fill, PathFillType::Winding)))?;
        self.restore()
    }
}
