use error::PdfCanvasError;
use pdf_canvas::PdfCanvas;
use pdf_object::ObjectVariant;
use pdf_operator::pdf_operator_backend::{
    ClippingPathOps, ColorOps, GraphicsStateOps, MarkedContentOps, PdfOperatorBackend,
    PdfOperatorBackendError, ShadingOps, TextObjectOps, TextPositioningOps, TextShowingOps,
    TextStateOps, XObjectOps,
};
use pdf_path::PdfPath;
use transform::Transform;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

pub mod canvas_path_ops;
pub mod error;
pub mod pdf_canvas;
pub mod pdf_path;
pub mod transform;

#[derive(Default, Clone)]
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
    fn draw_path(&mut self, path: &PdfPath, mode: PaintMode, fill_type: PathFillType);

    fn width(&self) -> f32;

    fn height(&self) -> f32;
}

impl<'a> PdfOperatorBackend for PdfCanvas<'a> {}

impl<'a> ClippingPathOps for PdfCanvas<'a> {
    fn clip_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn clip_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        todo!()
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
        todo!()
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
        todo!()
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
        todo!()
    }

    fn set_non_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        todo!()
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
        self.text_matrix = Transform::identity();
        self.text_line_matrix = Transform::identity();

        Ok(())
    }

    fn end_text_object(&mut self) -> Result<(), Self::ErrorType> {
        Ok(())
    }
}

impl<'a> TextStateOps for PdfCanvas<'a> {
    fn set_character_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.text_character_spacing = spacing;
        Ok(())
    }

    fn set_word_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.text_word_spacing = spacing;
        Ok(())
    }

    fn set_horizontal_text_scaling(&mut self, scale_percent: f32) -> Result<(), Self::ErrorType> {
        self.text_horizontal_scaling = scale_percent;
        Ok(())
    }

    fn set_text_leading(&mut self, leading: f32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_font_and_size(&mut self, font_name: &str, size: f32) -> Result<(), Self::ErrorType> {
        println!("set_font_and_size name: {} size: {}", font_name, size);
        self.text_font_size = size;

        if let Some(resources) = &self.page.resources {
            if let Some(font) = resources.fonts.get(font_name) {
                if let Some(font_file) = &font.cid_font.descriptor.font_file {
                    if let ObjectVariant::Stream(s) = &font_file {
                        let face =
                            Face::parse(s.data.as_slice(), 0).expect("Failed to parse font face");
                        self.font_face = Some(face);
                        self.text_word_spacing = 0.0;
                    }
                }

                self.current_font = Some(font);
            } else {
                panic!();
            }
        }
        Ok(())
    }

    fn set_text_rendering_mode(&mut self, mode: i32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_text_rise(&mut self, rise: f32) -> Result<(), Self::ErrorType> {
        self.text_rise = rise;
        Ok(())
    }
}

impl<'a> TextPositioningOps for PdfCanvas<'a> {
    fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), Self::ErrorType> {
        println!("move_text_position tx: {} ty: {}", tx, ty);
        self.text_matrix.translate(tx, ty);
        Ok(())
    }

    fn move_text_position_and_set_leading(
        &mut self,
        tx: f32,
        ty: f32,
    ) -> Result<(), Self::ErrorType> {
        todo!()
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
        todo!()
    }

    fn move_to_start_of_next_line(&mut self) -> Result<(), Self::ErrorType> {
        todo!()
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
        let current_font = self.current_font.ok_or(PdfCanvasError::NoCurrentFont)?;

        let face = self
            .font_face
            .as_ref()
            .ok_or(PdfCanvasError::NoCurrentFont)?;

        let horizontal_scaling = self.text_horizontal_scaling;
        let mut text_rendering_matrix = self.text_rendering_matrix();
        // TrueType fonts are prescaled to text_rendering_matrix.x_scale() * text_state().font_size / horizontal_scaling,
        // cf `Renderer::text_set_font()`. That's the width we get back from `get_glyph_width()` if we use a fallback
        // (or built-in) font. Scale the width size too, so the m_width.get() codepath is consistent.
        let font_size = text_rendering_matrix.sx * self.text_font_size / horizontal_scaling;

        let character_spacing = self.text_character_spacing;
        let word_spacing = self.text_word_spacing;

        let units_per_em = face.units_per_em() as f32;
        let scale = self.text_font_size / units_per_em;

        let mut transform = self.current_state().transform.clone();

        transform.scale(scale, scale);

        for cid in text {
            let glyph_id = GlyphId(*cid as u16);
            let mut builder = PdfGlyphOutline::new(transform.clone());
            face.outline_glyph(glyph_id, &mut builder);
            self.canvas
                .draw_path(&builder.path, PaintMode::Fill, PathFillType::EvenOdd);

            // FIGURE 5.5 Metrics for horizontal and vertical writing modes

            // Use the width specified in the font's dictionary if available,
            // and use the default width for the given font otherwise.

            let glyph_width = if let Some(width) = current_font
                .cid_font
                .widths
                .as_ref()
                .unwrap()
                .get_width(*cid as i64)
            {
                font_size * width / 1000.0
            } else {
                font_size * current_font.cid_font.descriptor.missing_width as f32 / 1000.0
            };

            // 'advance_user_units' is the glyph's horizontal advance in PDF user space units.
            // let advance_user_units = face.glyph_hor_advance(glyph_id).unwrap() as f32 * scale;
            let advance_user_units = glyph_width;
            // Transform this user space advance to a canvas space advance vector using the CTM's linear components.
            let canvas_advance_x = self.current_state().transform.sx * advance_user_units;
            // self.transform.ky is 0 for the typical CTM setup, so canvas_advance_y is 0.
            transform.translate(canvas_advance_x, 0.0);
        }

        Ok(())
    }

    fn show_text_with_glyph_positioning(
        &mut self,
        elements: &[pdf_operator::TextElement],
    ) -> Result<(), Self::ErrorType> {
        //for (auto& element : elements) {
        //    if (element.has_number()) {
        //        float shift = element.to_float() / 1000.0f;
        //        if (text_state().font->writing_mode() == WritingMode::Horizontal)
        //            m_text_matrix.translate(-shift * text_state().font_size * text_state().horizontal_scaling, 0.0f);
        //        else
        //            m_text_matrix.translate(0.0f, -shift * text_state().font_size);
        //        m_text_rendering_matrix_is_dirty = true;
        //    } else {
        //        auto str = element.get<NonnullRefPtr<Object>>()->cast<StringObject>()->string();
        //        TRY(show_text(str));
        //    }
        //}

        todo!()
    }

    fn move_to_next_line_and_show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_spacing_and_show_text(
        &mut self,
        word_spacing: f32,
        char_spacing: f32,
        text: &[u8],
    ) -> Result<(), Self::ErrorType> {
        todo!()
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
        tag: &str,
        properties_name_or_dict: &str,
    ) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn end_marked_content(&mut self) -> Result<(), Self::ErrorType> {
        todo!()
    }
}

impl<'a> PdfCanvas<'a> {
    /// Helper function to reduce repetition in path painting operations
    fn paint_taken_path(
        &mut self,
        mode: PaintMode,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        if let Some(path) = self.current_path.take() {
            self.canvas.draw_path(&path, mode, fill_type);
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }
}

impl PdfOperatorBackendError for PdfCanvas<'_> {
    type ErrorType = PdfCanvasError;
}
