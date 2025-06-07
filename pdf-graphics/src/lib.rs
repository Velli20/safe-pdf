use std::rc::Rc;

use color::Color;
use error::PdfCanvasError;
use pdf_canvas::PdfCanvas;
use pdf_content_stream::pdf_operator_backend::{
    ClippingPathOps, ColorOps, GraphicsStateOps, MarkedContentOps, PdfOperatorBackend,
    PdfOperatorBackendError, ShadingOps, TextObjectOps, TextPositioningOps, TextShowingOps,
    TextStateOps, XObjectOps,
};
use pdf_object::{ObjectVariant, dictionary::Dictionary};
use pdf_page::external_graphics_state::ExternalGraphicsStateKey;
use pdf_path::PdfPath;
use transform::Transform;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

pub mod canvas_path_ops;
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
}

impl<'a> PdfOperatorBackend for PdfCanvas<'a> {}

impl<'a> ClippingPathOps for PdfCanvas<'a> {
    fn clip_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        if let Some(path) = self.current_path.take() {
            self.canvas.set_clip_region(&path, PathFillType::Winding);
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }

    fn clip_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        if let Some(path) = self.current_path.take() {
            self.canvas.set_clip_region(&path, PathFillType::EvenOdd);
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }
}

impl<'a> GraphicsStateOps for PdfCanvas<'a> {
    fn save_graphics_state(&mut self) -> Result<(), Self::ErrorType> {
        self.save();
        Ok(())
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
        let ctm_old = self.current_state().transform.clone();
        let mut ctm_new = mat;
        ctm_new.concat(&ctm_old);
        self.current_state_mut().transform = ctm_new;
        Ok(())
    }

    fn set_line_width(&mut self, width: f32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_line_cap(&mut self, cap_style: i32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_line_join(&mut self, join_style: i32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_miter_limit(&mut self, limit: f32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_dash_pattern(
        &mut self,
        dash_array: &[f32],
        dash_phase: f32,
    ) -> Result<(), Self::ErrorType> {
        todo!()
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
                            self.current_state_mut().stroke_color.a = *alpha
                        }
                        ExternalGraphicsStateKey::NonStrokingAlpha(alpha) => {
                            self.current_state_mut().fill_color.a = *alpha
                        }
                    }
                }
            } else {
                panic!()
            }
        } else {
            panic!()
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
        todo!()
    }

    fn set_non_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut().stroke_color = Color::from_rgb(r, g, b);
        Ok(())
    }

    fn set_non_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut().fill_color = Color::from_rgb(r, g, b);
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

impl<'a> TextObjectOps for PdfCanvas<'a> {
    fn begin_text_object(&mut self) -> Result<(), Self::ErrorType> {
        self.current_state_mut().text_state.matrix = Transform::identity();
        self.current_state_mut().text_state.line_matrix = Transform::identity();

        Ok(())
    }

    fn end_text_object(&mut self) -> Result<(), Self::ErrorType> {
        Ok(())
    }
}

impl<'a> TextStateOps for PdfCanvas<'a> {
    fn set_character_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut().text_state.character_spacing = spacing;
        Ok(())
    }

    fn set_word_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut().text_state.word_spacing = spacing;
        Ok(())
    }

    fn set_horizontal_text_scaling(&mut self, scale_percent: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut().text_state.horizontal_scaling = scale_percent;
        Ok(())
    }

    fn set_text_leading(&mut self, leading: f32) -> Result<(), Self::ErrorType> {
        todo!("Implement text leading TL: {}", leading)
    }

    fn set_font_and_size(&mut self, font_name: &str, size: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut().text_state.font_size = size;

        let resources = self
            .page
            .resources
            .as_ref()
            .ok_or(PdfCanvasError::MissingPageResources)?;

        let font = resources
            .fonts
            .get(font_name)
            .ok_or(PdfCanvasError::FontNotFound(font_name.to_string()))?;

        if let Some(font_file) = &font.cid_font.descriptor.font_file {
            if let ObjectVariant::Stream(s) = &font_file {
                let face = Face::parse(s.data.as_slice(), 0).expect("Failed to parse font face");

                self.current_state_mut().text_state.font_face = Some(face);
                self.current_state_mut().text_state.word_spacing = 0.0; // Tj operator spec: "word spacing is applied to every occurrence of the single-byte character code 32 in a string when using a simple font or a composite font that defines code 32 as a space."
            }
        }

        self.current_state_mut().text_state.font = Some(font);
        Ok(())
    }

    fn set_text_rendering_mode(&mut self, mode: i32) -> Result<(), Self::ErrorType> {
        todo!("Implement text rendering mode Tr: {}", mode)
    }

    fn set_text_rise(&mut self, rise: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut().text_state.rise = rise;
        Ok(())
    }
}

impl<'a> TextPositioningOps for PdfCanvas<'a> {
    fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), Self::ErrorType> {
        let mat = Transform::from_translate(tx, ty);
        self.current_state_mut().text_state.line_matrix.concat(&mat);
        self.current_state_mut().text_state.matrix =
            self.current_state().text_state.line_matrix.clone();
        Ok(())
    }

    fn move_text_position_and_set_leading(
        &mut self,
        tx: f32,
        ty: f32,
    ) -> Result<(), Self::ErrorType> {
        todo!("Implement TD operator: tx={}, ty={}", tx, ty)
    }

    fn set_text_matrix(
        &mut self,
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
    ) -> Result<(), Self::ErrorType> {
        let mat = Transform::from_row(a, b, c, d, e, f);
        self.current_state_mut().text_state.line_matrix = mat.clone(); // text_line_matrix is also set
        self.current_state_mut().text_state.matrix = mat;
        Ok(())
    }

    fn move_to_start_of_next_line(&mut self) -> Result<(), Self::ErrorType> {
        todo!("Implement T* operator")
    }
}

#[derive(Default)]
struct PdfGlyphOutline {
    path: PdfPath,
    transform: Transform,
}

impl PdfGlyphOutline {
    pub fn new(transform: Transform) -> Self {
        Self {
            path: PdfPath::default(),
            transform,
        }
    }
}

impl OutlineBuilder for PdfGlyphOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform.transform_point(x, y);
        self.path.move_to(x, y).unwrap();
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform.transform_point(x, y);
        self.path.line_to(x, y).unwrap();
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.transform.transform_point(x1, y1);
        let (x, y) = self.transform.transform_point(x, y);
        self.path.quad_to(x1, y1, x, y).unwrap()
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.transform.transform_point(x1, y1);
        let (x2, y2) = self.transform.transform_point(x2, y2);
        let (x, y) = self.transform.transform_point(x, y);
        self.path.curve_to(x1, y1, x2, y2, x, y).unwrap();
    }

    fn close(&mut self) {
        self.path.close().unwrap();
    }
}

impl<'a> TextShowingOps for PdfCanvas<'a> {
    fn show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        let text_state = &self.current_state().text_state.clone();
        let current_font = text_state.font.ok_or(PdfCanvasError::NoCurrentFont)?;
        let face = text_state
            .font_face
            .as_ref()
            .ok_or(PdfCanvasError::NoCurrentFont)?;

        // Extract font and text state parameters.
        let units_per_em_f32 = face.units_per_em() as f32;
        let char_spacing = text_state.character_spacing;
        let word_spacing = text_state.word_spacing;
        let text_font_size = text_state.font_size;
        let text_rise = text_state.rise;

        // Compute the inverse of units per em for scaling.
        let upe_inv = if units_per_em_f32 != 0.0 {
            1.0 / units_per_em_f32
        } else {
            0.0
        };

        // Th_factor: Horizontal scaling factor (Th / 100).
        let th_factor = text_state.horizontal_scaling / 100.0;

        // Build the text rendering transform for this glyph:
        // - sx: horizontal scale (font size, units per em, horizontal scaling)
        // - sy: vertical scale (font size, units per em)
        // - ty: vertical offset (text rise)
        let m_params = Transform::from_row(
            text_font_size * upe_inv * th_factor, // sx
            0.0,                                  // ky (skew)
            0.0,                                  // kx (skew)
            text_font_size * upe_inv,             // sy
            0.0,                                  // tx
            text_rise,                            // ty
        );

        let fill_color = self.current_state().fill_color;
        // Iterate over each character in the input text.
        for char_code_byte in text {
            let char_code = *char_code_byte;
            // Skip characters not present in the font.
            if face.glyph_index(char_code as char).is_none() {
                continue;
            }

            let glyph_id = GlyphId(char_code as u16);

            // Compose the final transformation matrix for this glyph:
            // m_params -> text matrix -> current transformation matrix
            let mut glyph_matrix_for_char = m_params.clone();
            glyph_matrix_for_char.concat(&self.current_state().text_state.matrix);
            glyph_matrix_for_char.concat(&self.current_state().transform);

            // Build the glyph outline using the composed transform.
            let mut builder = PdfGlyphOutline::new(glyph_matrix_for_char);

            face.outline_glyph(glyph_id, &mut builder);

            // Fill it on the canvas
            self.canvas
                .fill_path(&builder.path, PathFillType::Winding, fill_color);

            // Determine the glyph's advance width in font units.
            let w0_glyph_units = current_font
                .cid_font
                .widths
                .as_ref()
                .and_then(|w_array| w_array.get_width(char_code as i64))
                .unwrap_or_else(|| current_font.cid_font.default_width as f32);

            // Convert width from font units to ems.
            let w0_ems = w0_glyph_units / 1000.0;

            // Scale the glyph width by the font size.
            let glyph_width_tfs_scaled = w0_ems * text_font_size;

            // Apply word spacing only to space characters.
            let word_spacing_for_char = if char_code == 32 { word_spacing } else { 0.0 };

            // Compute the horizontal advance for this glyph.
            let advance_x =
                (glyph_width_tfs_scaled + char_spacing + word_spacing_for_char) * th_factor;

            // Advance the text matrix for the next glyph.
            self.current_state_mut()
                .text_state
                .matrix
                .translate(advance_x, 0.0);
        }

        Ok(())
    }

    fn show_text_with_glyph_positioning(
        &mut self,
        elements: &[pdf_content_stream::TextElement],
    ) -> Result<(), Self::ErrorType> {
        todo!("Implement TJ operator: {:?}", elements)
    }

    fn move_to_next_line_and_show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        todo!("Implement ' operator: {:?}", text)
    }

    fn set_spacing_and_show_text(
        &mut self,
        word_spacing: f32,
        char_spacing: f32,
        text: &[u8],
    ) -> Result<(), Self::ErrorType> {
        todo!(
            "Implement \" operator: word_spacing={}, char_spacing={}, text={:?}",
            word_spacing,
            char_spacing,
            text
        )
    }
}

impl<'a> XObjectOps for PdfCanvas<'a> {
    fn invoke_xobject(&mut self, xobject_name: &str) -> Result<(), Self::ErrorType> {
        todo!()
    }
}

impl<'a> ShadingOps for PdfCanvas<'a> {
    fn paint_shading(&mut self, shading_name: &str) -> Result<(), Self::ErrorType> {
        todo!()
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
