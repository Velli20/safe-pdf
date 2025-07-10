use pdf_content_stream::{
    graphics_state_operators::{LineCap, LineJoin},
    pdf_operator_backend::GraphicsStateOps,
};
use pdf_page::{external_graphics_state::ExternalGraphicsStateKey, xobject::XObject};

use crate::{
    canvas::Canvas, canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
    transform::Transform,
};

impl<'a, T: CanvasBackend> GraphicsStateOps for PdfCanvas<'a, T> {
    fn save_graphics_state(&mut self) -> Result<(), Self::ErrorType> {
        self.save()
    }

    fn restore_graphics_state(&mut self) -> Result<(), Self::ErrorType> {
        self.restore()
    }

    fn concat_matrix(
        &mut self,
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
    ) -> Result<(), Self::ErrorType> {
        let mat = Transform::from_row(a, b, c, d, e, f);
        let ctm_old = self.current_state()?.transform.clone();
        let mut ctm_new = mat;
        ctm_new.concat(&ctm_old);
        self.current_state_mut()?.transform = ctm_new;
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
        println!("Dash pattern");
        Ok(())
    }

    fn set_rendering_intent(&mut self, intent: &str) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_flatness_tolerance(&mut self, tolerance: f32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_graphics_state_from_dict(&mut self, dict_name: &str) -> Result<(), Self::ErrorType> {
        let resources = self.get_resources()?;

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
                    self.current_state_mut()?.line_cap = LineCap::from(*cap as u8)
                }
                ExternalGraphicsStateKey::LineJoin(join) => {
                    self.current_state_mut()?.line_join = LineJoin::from(*join as u8)
                }
                ExternalGraphicsStateKey::MiterLimit(miter) => {
                    self.current_state_mut()?.miter_limit = *miter;
                }
                ExternalGraphicsStateKey::DashPattern(..) => todo!(),
                ExternalGraphicsStateKey::RenderingIntent(_) => todo!(),
                ExternalGraphicsStateKey::OverprintStroke(_) => todo!(),
                ExternalGraphicsStateKey::OverprintFill(_) => todo!(),
                ExternalGraphicsStateKey::OverprintMode(_) => todo!(),
                ExternalGraphicsStateKey::Font(..) => todo!(),
                ExternalGraphicsStateKey::BlendMode(_) => {
                    // println!("Blend mode {:?}", items);
                }
                ExternalGraphicsStateKey::SoftMask(smask) => {
                    if let Some(smask) = smask {
                        if let XObject::Image(_) = &smask.shape {
                            panic!()
                        } else if let XObject::Form(form) = &smask.shape {
                            let mut mask = self.canvas.create_mask(
                                form.bbox[2] - form.bbox[0],
                                form.bbox[3] - form.bbox[1],
                            );

                            let mut other =
                                PdfCanvas::new(mask.as_mut(), self.page, Some(&form.bbox));
                            other.render_form_xobject(form)?;

                            self.canvas.enable_mask(mask.as_mut());
                            self.mask = Some(mask);
                        }
                    } else {
                        if let Some(mut mask) = self.mask.take() {
                            let transform = self.current_state()?.transform.clone();
                            self.canvas.finish_mask(mask.as_mut(), &transform);
                        }
                    }
                }
                ExternalGraphicsStateKey::StrokingAlpha(alpha) => {
                    self.current_state_mut()?.stroke_color.a = *alpha
                }
                ExternalGraphicsStateKey::NonStrokingAlpha(alpha) => {
                    self.current_state_mut()?.fill_color.a = *alpha
                }
            }
        }
        Ok(())
    }
}
