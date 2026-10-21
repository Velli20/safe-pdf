//! PDF graphics-state selection and save/restore operators.
use std::sync::Arc;

use pdf_graphics::{DashPattern, LineCap, LineJoin, MaskMode, transform::Transform};
use pdf_resources::{external_graphics_state::ExternalGraphicsStateKey, resource::Resource};

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Concatenates the supplied matrix with the current PDF transformation.
    pub(crate) fn concat_matrix(&mut self, transform: &Transform) -> Result<(), PdfCanvasError> {
        // PDF 'cm' operator: update the current transformation matrix (CTM) by
        // concatenating the provided matrix [a b c d e f] onto the current CTM.
        //
        // With our `Transform` convention, `post_concat` performs a post-multiply:
        //   CTM_new = CTM_old × M_incoming
        self.current_state_mut()?.transform.post_concat(transform);
        Ok(())
    }

    /// Updates the width used for subsequent path strokes.
    pub(crate) fn set_line_width(&mut self, width: f32) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.paint.line_width = width;
        Ok(())
    }

    /// Updates the end-cap style used for subsequent path strokes.
    pub(crate) fn set_line_cap(&mut self, cap_style: LineCap) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.paint.line_cap = cap_style;
        Ok(())
    }

    /// Updates the corner-join style used for subsequent path strokes.
    pub(crate) fn set_line_join(&mut self, line_join: LineJoin) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.paint.line_join = line_join;
        Ok(())
    }

    /// Updates the maximum miter length relative to stroke width.
    pub(crate) fn set_miter_limit(&mut self, miter_limit: f32) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.paint.miter_limit = miter_limit;
        Ok(())
    }

    /// Validates and stores dash intervals and their starting phase.
    pub(crate) fn set_dash_pattern(
        &mut self,
        dash_array: &[f32],
        dash_phase: f32,
    ) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.paint.dash_pattern = DashPattern::new(dash_array, dash_phase)?;
        Ok(())
    }

    /// Applies extended graphics-state entries, preparing soft-mask recordings when selected.
    pub(crate) fn set_graphics_state_from_dict(
        &mut self,
        dict_name: &[u8],
    ) -> Result<(), PdfCanvasError> {
        let resources = self
            .current_state()?
            .resources
            .clone()
            .ok_or(PdfCanvasError::PageResourcesMissing)?;

        let Some(states) = resources.external_graphics_state(dict_name) else {
            // If the specified `ExtGState` is not found, the ignored parameters should not cause an error.
            return Ok(());
        };

        for state in &states.params {
            match state {
                ExternalGraphicsStateKey::LineWidth(width) => {
                    self.current_state_mut()?.paint.line_width = *width
                }
                ExternalGraphicsStateKey::LineCap(cap) => {
                    self.current_state_mut()?.paint.line_cap = *cap;
                }
                ExternalGraphicsStateKey::LineJoin(join) => {
                    self.current_state_mut()?.paint.line_join = *join;
                }
                ExternalGraphicsStateKey::MiterLimit(miter) => {
                    self.current_state_mut()?.paint.miter_limit = *miter;
                }
                ExternalGraphicsStateKey::DashPattern(dash_pattern) => {
                    self.current_state_mut()?.paint.dash_pattern = Some(dash_pattern.clone());
                }
                ExternalGraphicsStateKey::Font(font, font_size) => {
                    let resource = font.get()?;
                    if let Resource::Font { font, resources } = resource.as_ref() {
                        let handle = self.load_pdf_font(font.as_ref())?;
                        self.current_state_mut()?.text_state.font = Some(handle);
                        self.current_state_mut()?.text_state.font_spec = Some(Arc::clone(font));
                        if let Some(resources) = resources {
                            self.current_state_mut()?.text_state.resources = Some(resources.get()?);
                        }
                    } else {
                        return Err(PdfCanvasError::UnsupportedFeature(
                            "ExtGState: Font resource is not a font".into(),
                        ));
                    }

                    self.current_state_mut()?.text_state.style.font_size = *font_size;
                }
                ExternalGraphicsStateKey::BlendMode(mode) => {
                    self.current_state_mut()?.paint.blend_mode = Some(mode.clone());
                }
                ExternalGraphicsStateKey::SoftMask(smask) => {
                    // Handle the `/SMask` entry from an `ExtGState` dictionary.
                    if let Some(smask) = smask.as_ref() {
                        let smask = smask.get()?;
                        if matches!(&smask.mask_type, MaskMode::Unknown(_)) {
                            continue;
                        }

                        let form = smask.shape.get()?;
                        // The soft mask is defined by a Form XObject.
                        // We need to render this form's content into a separate mask surface.
                        // Without bounds there is no offscreen surface to record into.
                        let Some(bbox) = form.bbox.as_ref() else {
                            continue;
                        };
                        if !Self::can_record_offscreen_bbox(bbox) {
                            continue;
                        }

                        // Render the form's content stream into the mask canvas.
                        let recording_canvas = self.record_content_stream(
                            &form.content_stream,
                            form.matrix,
                            bbox,
                            form.resources
                                .as_ref()
                                .map(|resources| resources.get())
                                .transpose()?,
                            None,
                            None,
                        )?;

                        let transform = self.current_state()?.transform;

                        self.current_state_mut()?.soft_mask =
                            Some(crate::mask_layer::MaskLayer::new(
                                Arc::new(recording_canvas),
                                transform,
                                smask.mask_type.clone(),
                                smask.transfer.table().cloned(),
                            )?);
                    } else {
                        self.current_state_mut()?.soft_mask = None;
                    }
                }
                ExternalGraphicsStateKey::StrokingAlpha(alpha) => {
                    self.current_state_mut()?.paint.stroke_color.a = *alpha
                }
                ExternalGraphicsStateKey::NonStrokingAlpha(alpha) => {
                    self.current_state_mut()?.paint.fill_color.a = *alpha
                }
            }
        }
        Ok(())
    }
}
