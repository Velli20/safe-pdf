use std::sync::Arc;

use pdf_content_stream::pdf_operator_backend::GraphicsStateOps;
use pdf_graphics::{LineCap, LineJoin, transform::Transform};
use pdf_page::{external_graphics_state::ExternalGraphicsStateKey, xobject::XObject};

use crate::{
    canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
    recording_canvas::RecordingCanvas,
};

impl<B: CanvasBackend> GraphicsStateOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn save_graphics_state(&mut self) -> Result<(), Self::ErrorType> {
        self.save()
    }

    fn restore_graphics_state(&mut self) -> Result<(), Self::ErrorType> {
        self.restore()
    }

    fn concat_matrix(&mut self, transform: &Transform) -> Result<(), Self::ErrorType> {
        // PDF 'cm' operator: update the current transformation matrix (CTM) by
        // concatenating the provided matrix [a b c d e f] onto the current CTM.
        //
        // With our `Transform` convention, `post_concat` performs a post-multiply:
        //   CTM_new = CTM_old × M_incoming
        self.current_state_mut()?.transform.post_concat(transform);
        Ok(())
    }

    fn set_line_width(&mut self, width: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.line_width = width;
        Ok(())
    }

    fn set_line_cap(&mut self, cap_style: LineCap) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.line_cap = cap_style;
        Ok(())
    }

    fn set_line_join(&mut self, line_join: LineJoin) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.line_join = line_join;
        Ok(())
    }

    fn set_miter_limit(&mut self, miter_limit: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.miter_limit = miter_limit;
        Ok(())
    }

    fn set_dash_pattern(
        &mut self,
        _dash_array: &[f32],
        _dash_phase: f32,
    ) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn set_rendering_intent(&mut self, _intent: &str) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn set_flatness_tolerance(&mut self, _tolerance: f32) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn set_graphics_state_from_dict(&mut self, dict_name: &str) -> Result<(), Self::ErrorType> {
        let resources = self
            .current_state()?
            .resources
            .ok_or(PdfCanvasError::MissingPageResources)?;

        let states = resources
            .external_graphics_states
            .get(dict_name)
            .ok_or_else(|| PdfCanvasError::GraphicsStateNotFound(dict_name.to_string()))?;

        for state in &states.params {
            match state {
                ExternalGraphicsStateKey::LineWidth(width) => {
                    self.current_state_mut()?.line_width = *width
                }
                ExternalGraphicsStateKey::LineCap(cap) => {
                    self.current_state_mut()?.line_cap = *cap;
                }
                ExternalGraphicsStateKey::LineJoin(join) => {
                    self.current_state_mut()?.line_join = *join;
                }
                ExternalGraphicsStateKey::MiterLimit(miter) => {
                    self.current_state_mut()?.miter_limit = *miter;
                }
                ExternalGraphicsStateKey::DashPattern(..) => {
                    return Err(PdfCanvasError::NotImplemented(
                        "ExtGState: DashPattern".into(),
                    ));
                }
                ExternalGraphicsStateKey::RenderingIntent(_) => {
                    return Err(PdfCanvasError::NotImplemented(
                        "ExtGState: RenderingIntent".into(),
                    ));
                }
                ExternalGraphicsStateKey::OverprintStroke(_) => {}
                ExternalGraphicsStateKey::OverprintFill(_) => {}
                ExternalGraphicsStateKey::OverprintMode(_) => {}
                ExternalGraphicsStateKey::Font(..) => {
                    return Err(PdfCanvasError::NotImplemented("ExtGState: Font".into()));
                }
                ExternalGraphicsStateKey::BlendMode(modes) => {
                    // Store the blend mode(s) in the current graphics state.
                    // PDF spec: If multiple blend modes are specified, use the first one supported.
                    // We only support the first for now.
                    if modes.len() > 1 {
                        return Err(PdfCanvasError::NotImplemented(
                            "ExtGState: Only one blend mode is supported".into(),
                        ));
                    }
                    if let Some(mode) = modes.first() {
                        self.current_state_mut()?.blend_mode = Some(*mode);
                    }
                }
                ExternalGraphicsStateKey::SoftMask(smask) => {
                    // Handle the `/SMask` entry from an `ExtGState` dictionary.
                    if let Some(smask) = smask.as_ref() {
                        if let XObject::Image(_) = &smask.shape {
                            return Err(PdfCanvasError::NotImplemented(
                                "SoftMask with Image shape".into(),
                            ));
                        } else if let XObject::Form(form) = &smask.shape {
                            // The soft mask is defined by a Form XObject.
                            // We need to render this form's content into a separate mask surface.

                            // Create a recording canvas to act as the mask layer.
                            let mut recording_canvas =
                                RecordingCanvas::new(form.bbox.width(), form.bbox.height());

                            // Render the form's content stream into the mask canvas.
                            self.record_content_stream(
                                &mut recording_canvas,
                                &form.content_stream.operations,
                                form.matrix,
                                &form.bbox,
                                form.resources.as_ref(),
                                None,
                            )?;

                            let transform = self.current_state()?.transform;

                            let arc = Arc::new(recording_canvas);

                            // Enable the mask on the main canvas. Subsequent drawing operations
                            // will be modulated by this mask.
                            self.canvas
                                .begin_mask_layer(&arc, &transform, smask.mask_type)?;

                            // Store the mask in the current canvas state to be used until it's finished.
                            self.mask = Some((Arc::clone(&arc), smask.mask_type, transform));
                        }
                    } else if let Some((mask, mask_type, transform)) = self.mask.take() {
                        // This branch handles the case where `/SMask` is set to `/None` in the `ExtGState`,
                        // which signals the end of the current soft mask application.
                        self.canvas.end_mask_layer(&mask, &transform, mask_type)?;
                    }
                }
                ExternalGraphicsStateKey::StrokingAlpha(alpha) => {
                    self.current_state_mut()?.stroke_color.a = *alpha
                }
                ExternalGraphicsStateKey::NonStrokingAlpha(alpha) => {
                    self.current_state_mut()?.fill_color.a = *alpha
                }
                ExternalGraphicsStateKey::StrokeAdjustment(_) => {}
                ExternalGraphicsStateKey::AppleAntiAliasing(_) => {}
                ExternalGraphicsStateKey::AlphaIsShape(_) => {}
                ExternalGraphicsStateKey::SmoothnessTolerance(_) => {}
                ExternalGraphicsStateKey::TransferFunction => {}
                ExternalGraphicsStateKey::TransferFunctionNew => {}
            }
        }
        Ok(())
    }
}
