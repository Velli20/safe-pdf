//! Soft-mask recording for an `ExtGState` `/SMask`.
use std::sync::Arc;

use pdf_graphics::{MaskMode, color::Color, rect::Rect, size::Size, transform::Transform};
use pdf_resources::soft_mask::SoftMask;

use crate::{
    canvas_backend::CanvasBackend, canvas_state::CanvasState, error::PdfCanvasError,
    mask_layer::MaskLayer, pdf_canvas::PdfCanvas, text_state::TextState,
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
        if !Self::can_record_offscreen_bbox(bbox) || !Self::can_record_offscreen_bbox(&bounds) {
            return Ok(None);
        }
        let backdrop = match smask.mask_type {
            MaskMode::Luminosity => smask.backdrop,
            _ => None,
        };
        // Without a backdrop, coverage outside the group's bounds is zero, so the
        // recording needs to cover only those bounds.
        let region = match backdrop {
            Some(_) => self
                .visible_user_area()?
                .filter(Self::can_record_offscreen_bbox)
                .unwrap_or(bounds),
            None => bounds,
        };
        let resources = form
            .resources
            .as_ref()
            .map(|resources| resources.get())
            .transpose()?;
        let initial_state = CanvasState {
            transform: Transform::identity(),
            pattern_parent_transform: Transform::identity(),
            text_state: TextState::default(),
            ..Default::default()
        };
        let size = region.size();
        // Backends cover only `[0, w] x [0, h]` of mask space, so mask space starts at
        // the region's origin in the current user space.
        let recording = self.record_nested(size, initial_state, None, |canvas| {
            if let Some(color) = backdrop {
                canvas.paint_backdrop(color, size)?;
            }
            canvas.render_content_stream(
                &form.content_stream,
                Some(
                    Transform::from_translate(-region.left, -region.top).post_concatenated(&matrix),
                ),
                Some(bbox),
                resources,
                None,
            )
        })?;
        let transform = self
            .current_state()?
            .transform
            .post_concatenated(&Transform::from_translate(region.left, region.top));
        Ok(Some(MaskLayer::new(
            Arc::new(recording),
            transform,
            smask.mask_type.clone(),
            smask.transfer.table().cloned(),
        )?))
    }

    /// Maps the backend's visible area into the current user space, or `None` when the
    /// current transform is singular.
    fn visible_user_area(&self) -> Result<Option<Rect>, PdfCanvasError> {
        let device = Rect {
            left: 0.0,
            top: 0.0,
            right: self.canvas.width(),
            bottom: self.canvas.height(),
        };
        Ok(self
            .current_state()?
            .transform
            .try_inverse()
            .ok()
            .map(|inverse| inverse.map_rect(&device)))
    }

    /// Fills `[0, width] x [0, height]` with an opaque backdrop `color`, leaving the
    /// graphics state unchanged.
    fn paint_backdrop(&mut self, color: Color, size: Size<f32>) -> Result<(), PdfCanvasError> {
        self.save()?;
        self.current_state_mut()?.paint.set_fill_color(color);
        self.rectangle(0.0, 0.0, size.width, size.height)?;
        self.fill_path_nonzero_winding()?;
        self.restore()
    }
}
