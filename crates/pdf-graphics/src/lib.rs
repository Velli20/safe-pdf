use std::rc::Rc;

use color::Color;
use error::PdfCanvasError;
use pdf_canvas::PdfCanvas;
use pdf_content_stream::{graphics_state_operators::{LineCap, LineJoin}, pdf_operator_backend::{
    ClippingPathOps, ColorOps, GraphicsStateOps, MarkedContentOps, PdfOperatorBackend,
    PdfOperatorBackendError, ShadingOps, XObjectOps,
}};
use pdf_object::dictionary::Dictionary;
use pdf_page::external_graphics_state::ExternalGraphicsStateKey;
use pdf_path::PdfPath;
use transform::Transform;

pub mod canvas_path_ops;
pub mod canvas_text_ops;
pub mod color;
pub mod error;
pub mod pdf_canvas;
pub mod pdf_path;
pub mod transform;

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

pub trait CanvasBackend {
    fn fill_path(&mut self, path: &PdfPath, fill_type: PathFillType, color: Color);

    fn stroke_path(&mut self, path: &PdfPath, color: Color, line_width: f32);

    fn set_clip_region(&mut self, path: &PdfPath, mode: PathFillType);

    fn width(&self) -> f32;

    fn height(&self) -> f32;

    fn reset_clip(&mut self);
}

impl<'a> PdfOperatorBackend for PdfCanvas<'a> {}

impl<'a> ClippingPathOps for PdfCanvas<'a> {
    fn clip_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        if let Some(mut path) = self.current_path.take() {
            path.transform(&self.current_state()?.transform)?;
            self.canvas.set_clip_region(&path, PathFillType::Winding);
            self.current_state_mut()?.clip_path = Some(path);
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }

    fn clip_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        if let Some(mut path) = self.current_path.take() {
            path.transform(&self.current_state()?.transform)?;
            self.canvas.set_clip_region(&path, PathFillType::EvenOdd);
            self.current_state_mut()?.clip_path = Some(path);
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }
}

impl<'a> GraphicsStateOps for PdfCanvas<'a> {
    fn save_graphics_state(&mut self) -> Result<(), Self::ErrorType> {
        self.save()
    }

    fn restore_graphics_state(&mut self) -> Result<(), Self::ErrorType> {
        self.restore();
        Ok(())
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

    fn set_miter_limit(&mut self, limit: f32) -> Result<(), Self::ErrorType> {
        todo!()
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
        if let Some(resources) = self.page.resources.as_ref() {
            if let Some(states) = resources.external_graphics_states.get(dict_name) {
                for state in &states.params {
                    match state {
                        ExternalGraphicsStateKey::LineWidth(_) => todo!(),
                        ExternalGraphicsStateKey::LineCap(_) => todo!(),
                        ExternalGraphicsStateKey::LineJoin(_) => todo!(),
                        ExternalGraphicsStateKey::MiterLimit(_) => todo!(),
                        ExternalGraphicsStateKey::DashPattern(items, _) => todo!(),
                        ExternalGraphicsStateKey::RenderingIntent(_) => todo!(),
                        ExternalGraphicsStateKey::OverprintStroke(_) => todo!(),
                        ExternalGraphicsStateKey::OverprintFill(_) => todo!(),
                        ExternalGraphicsStateKey::OverprintMode(_) => todo!(),
                        ExternalGraphicsStateKey::Font(_, _) => todo!(),
                        ExternalGraphicsStateKey::BlendMode(items) => {
                            // println!("Blend mode {:?}", items);
                        }
                        ExternalGraphicsStateKey::SoftMask(dictionary) => todo!(),
                        ExternalGraphicsStateKey::StrokingAlpha(alpha) => {
                            self.current_state_mut()?.stroke_color.a = *alpha
                        }
                        ExternalGraphicsStateKey::NonStrokingAlpha(alpha) => {
                            self.current_state_mut()?.fill_color.a = *alpha
                        }
                    }
                }
            } else {
                panic!()
            }
        } else {
            // panic!()
        }
        Ok(())
    }
}

impl<'a> ColorOps for PdfCanvas<'a> {
    fn set_stroking_color_space(&mut self, name: &str) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_non_stroking_color_space(&mut self, name: &str) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: Option<&str>,
    ) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_non_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        todo!()
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

impl<'a> XObjectOps for PdfCanvas<'a> {
    fn invoke_xobject(&mut self, xobject_name: &str) -> Result<(), Self::ErrorType> {
        println!("Invoke xobject {:?}", xobject_name);
        Ok(())
    }
}

impl<'a> ShadingOps for PdfCanvas<'a> {
    fn paint_shading(&mut self, shading_name: &str) -> Result<(), Self::ErrorType> {
        println!("Paint shading {:?}", shading_name);
        Ok(())
    }
}

impl<'a> MarkedContentOps for PdfCanvas<'a> {
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

impl PdfOperatorBackendError for PdfCanvas<'_> {
    type ErrorType = PdfCanvasError;
}
