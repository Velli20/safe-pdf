use std::rc::Rc;

use color::Color;
use error::PdfCanvasError;
use pdf_canvas::PdfCanvas;
use pdf_content_stream::{
    graphics_state_operators::{LineCap, LineJoin},
    pdf_operator_backend::{
        ClippingPathOps, ColorOps, GraphicsStateOps, MarkedContentOps, PdfOperatorBackend,
        PdfOperatorBackendError, ShadingOps, XObjectOps,
    },
};
use pdf_object::dictionary::Dictionary;
use pdf_page::{
    external_graphics_state::ExternalGraphicsStateKey, image::ImageFilter, xobject::XObject,
};
use transform::Transform;

use crate::{canvas::Canvas, canvas_backend::CanvasBackend};

pub mod canvas;
pub mod canvas_backend;
pub mod canvas_path_ops;
pub mod canvas_text_ops;
pub mod color;
pub mod error;
pub mod pdf_canvas;
pub mod pdf_path;
pub mod text_renderer;
pub mod transform;
pub mod truetype_font_renderer;
pub mod type3_font_renderer;

#[derive(Default, Clone, PartialEq)]
pub enum PaintMode {
    #[default]
    Fill,
    Stroke,
    FillAndStroke,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PathFillType {
    /// Specifies that "inside" is computed by a non-zero sum of signed edge crossings
    #[default]
    Winding,
    /// Specifies that "inside" is computed by an odd number of edge crossings
    EvenOdd,
}

impl<'a, T: CanvasBackend> PdfOperatorBackend for PdfCanvas<'a, T> {}

impl<'a, T: CanvasBackend> ClippingPathOps for PdfCanvas<'a, T> {
    fn clip_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        if let Some(mut path) = self.current_path.take() {
            path.transform(&self.current_state()?.transform);
            self.canvas.set_clip_region(&path, PathFillType::Winding);
            self.current_state_mut()?.clip_path = Some(path);
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }

    fn clip_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        if let Some(mut path) = self.current_path.take() {
            path.transform(&self.current_state()?.transform);
            self.canvas.set_clip_region(&path, PathFillType::EvenOdd);
            self.current_state_mut()?.clip_path = Some(path);
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }
}

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
        dash_array: &[f32],
        dash_phase: f32,
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
                        if let XObject::Image(shape) = &smask.shape {
                            println!("smaks image width {} height {}", shape.width, shape.height);
                            panic!()
                        } else if let XObject::Form(form) = &smask.shape {
                            println!(
                                "smaks form bb left {} top {} right {} bottom {}",
                                form.bbox[0], form.bbox[1], form.bbox[2], form.bbox[3]
                            );

                            let mut mask = self.canvas.create_mask(
                                form.bbox[2] - form.bbox[0],
                                form.bbox[3] - form.bbox[1],
                            );

                            let mut other = PdfCanvas::new(
                                mask.as_mut(),
                                self.page,
                                Some(&form.bbox), /* None */
                            );
                            other.render_form_xobject(form)?;

                            let ss = self.current_state()?.transform.clone();
                            self.canvas.enable_mask(mask.as_mut(), &ss);
                            self.mask = Some(mask);
                        }
                    } else {
                        if let Some(mut mask) = self.mask.take() {
                            let ss = self.current_state()?.transform.clone();
                            self.canvas.finish_mask(mask.as_mut(), &ss);
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

impl<'a, T: CanvasBackend> ColorOps for PdfCanvas<'a, T> {
    fn set_stroking_color_space(&mut self, name: &str) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn set_non_stroking_color_space(&mut self, name: &str) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn set_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn set_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: Option<&str>,
    ) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn set_non_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn set_non_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: Option<&str>,
    ) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        println!("Set stroking gray {:?}", gray);
        Ok(())
    }

    fn set_non_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        println!("Non stroking gray {:?}", gray);
        Ok(())
    }

    fn set_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.stroke_color = Color::from_rgb(r, g, b);
        Ok(())
    }

    fn set_non_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.fill_color = Color::from_rgb(r, g, b);
        Ok(())
    }

    fn set_stroking_cmyk(&mut self, c: f32, m: f32, y: f32, k: f32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_non_stroking_cmyk(
        &mut self,
        c: f32,
        m: f32,
        y: f32,
        k: f32,
    ) -> Result<(), Self::ErrorType> {
        todo!()
    }
}

impl<'a, T: CanvasBackend> XObjectOps for PdfCanvas<'a, T> {
    fn invoke_xobject(&mut self, xobject_name: &str) -> Result<(), Self::ErrorType> {
        let resources = self.get_resources()?;

        if let Some(XObject::Image(image)) = resources.xobjects.get(xobject_name) {
            let smask = if let Some(m) = image.smask.as_ref() {
                Some(m.data.as_slice())
            } else {
                None
            };

            let mat = self.current_state()?.transform.clone();

            self.canvas.draw_image(
                &image.data,
                image.filter == Some(ImageFilter::DCTDecode),
                image.width as f32,
                image.height as f32,
                image.bits_per_component as u32,
                &mat,
                smask,
            );
        } else if let Some(XObject::Form(form)) = resources.xobjects.get(xobject_name) {
            self.render_form_xobject(form)?;
        } else {
            return Err(PdfCanvasError::XObjectNotFound(xobject_name.to_string()));
        }
        Ok(())
    }
}

impl<'a, T: CanvasBackend> ShadingOps for PdfCanvas<'a, T> {
    fn paint_shading(&mut self, shading_name: &str) -> Result<(), Self::ErrorType> {
        println!("Paint shading {:?}", shading_name);
        Ok(())
    }
}

impl<'a, T: CanvasBackend> MarkedContentOps for PdfCanvas<'a, T> {
    fn mark_point(&mut self, tag: &str) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn mark_point_with_properties(
        &mut self,
        tag: &str,
        properties_name_or_dict: &str,
    ) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn begin_marked_content(&mut self, tag: &str) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn begin_marked_content_with_properties(
        &mut self,
        _tag: &str,
        _properties: &Rc<Dictionary>,
    ) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn end_marked_content(&mut self) -> Result<(), Self::ErrorType> {
        Ok(())
    }
}

impl<T> PdfOperatorBackendError for PdfCanvas<'_, T> {
    type ErrorType = PdfCanvasError;
}
