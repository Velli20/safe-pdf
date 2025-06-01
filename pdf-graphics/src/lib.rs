use std::path;

use color::Color;
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
        self.text_font_size = size;

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

                self.font_face = Some(face);
                self.text_word_spacing = 0.0;
            }
        }

        self.current_font = Some(font);
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
        let mat = Transform::from_translate(tx, ty);
        self.text_line_matrix.concat(&mat);
        self.text_matrix = self.text_line_matrix.clone();
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

        // Text state parameters (PDF 1.7 Spec, Section 5.3.3)
        let units_per_em_f32 = face.units_per_em() as f32;
        let char_spacing = self.text_character_spacing; // Tc: Character spacing
        let word_spacing = self.text_word_spacing; // Tw: Word spacing
        let text_font_size = self.text_font_size; // Tfs: Text font size
        let text_rise = self.text_rise; // Ts: Text rise

        // Avoid division by zero if units_per_em is somehow zero, though unlikely for valid fonts.
        let upe_inv = if units_per_em_f32 != 0.0 {
            1.0 / units_per_em_f32
        } else {
            0.0
        }; // Inverse of units_per_em, for converting font units to 1-unit glyph space.

        // Th_factor: Horizontal scaling factor (Th / 100). (PDF Spec 5.3.3)
        let th_factor = self.text_horizontal_scaling / 100.0;

        // M_params: This matrix accounts for font size (Tfs), horizontal scaling (Th),
        // and text rise (Ts). It transforms glyph coordinates from font design units
        // (scaled by 1/units_per_em) into a text space that is appropriately scaled and shifted.
        // (PDF Spec 1.7, Section 5.3.1, Figure 46)
        // M_params = [ (Tfs/upe) * (Th/100)     0                     0 ]
        //            [ 0                        (Tfs/upe)             0 ]
        //            [ 0                        Ts (self.text_rise)   1 ]
        let m_params = Transform::from_row(
            text_font_size * upe_inv * th_factor, // sx = (Tfs/upe) * (Th/100)
            0.0,                                  // ky (skew)
            0.0,                                  // kx (skew)
            text_font_size * upe_inv,             // sy = Tfs/upe
            0.0,                                  // tx
            text_rise,                            // ty = Ts (text_rise)
        );

        for char_code_byte in text {
            let char_code = *char_code_byte;
            let glyph_id = GlyphId(char_code as u16);

            // Calculate the final glyph rendering matrix for the current glyph:
            // M_glyph = CTM * T_m * M_params
            // Where:
            //  - CTM is the Current Transformation Matrix (from graphics state)
            //  - T_m is the Text Matrix (self.text_matrix, updated by text positioning ops)
            //  - M_params incorporates Tfs, Th, Ts (calculated above)
            //
            // The 'concat' method performs pre-multiplication: A.concat(B) results in B * A.
            // 1. m_params.concat(&self.text_matrix) results in: T_m * M_params
            // 2. (T_m * M_params).concat(&self.current_state().transform) results in: CTM * T_m * M_params
            let mut glyph_matrix_for_char = m_params.clone();
            glyph_matrix_for_char.concat(&self.text_matrix);
            glyph_matrix_for_char.concat(&self.current_state().transform);

            let mut builder = PdfGlyphOutline::new(glyph_matrix_for_char);

            face.outline_glyph(glyph_id, &mut builder);

            // Render the glyph path. According to the specification, the
            // default fill-rule for text is non-zero winding rule.
            self.canvas.fill_path(
                &builder.path,
                PathFillType::Winding,
                self.current_state().fill_color,
            );

            // Calculate horizontal displacement (advance_x) for the current glyph in text space.
            // (PDF Spec 1.7, Section 5.3.2, Tj operator)
            //
            // w0: Glyph width in font design units (typically 1000ths of an em,
            //     obtained from /Widths array or /DW in the font dictionary).
            let w0_glyph_units = current_font
                .cid_font
                .widths
                .as_ref()
                .and_then(|w_array| w_array.get_width(char_code as i64))
                .unwrap_or_else(|| current_font.cid_font.default_width as f32);

            // Convert w0 to ems (PDF widths are often in 1000ths of a unit of text space).
            let w0_ems = w0_glyph_units / 1000.0;

            // Glyph width in text space, scaled by Tfs: (w0/1000) * Tfs.
            // This is before applying horizontal scaling (Th).
            let glyph_width_tfs_scaled = w0_ems * text_font_size;

            // Apply word spacing (Tw) if the character is a space (ASCII 32).
            let word_spacing_for_char = if char_code == 32 { word_spacing } else { 0.0 };

            // Total horizontal displacement tx for this glyph:
            // tx = ((w0_ems * Tfs) + Tc + Tw_for_char) * (Th/100)
            let advance_x =
                (glyph_width_tfs_scaled + char_spacing + word_spacing_for_char) * th_factor;
            // Update the text matrix T_m for the next glyph: T_m_new = Translate(advance_x, 0) * T_m_old
            self.text_matrix.translate(advance_x, 0.0);
        }

        Ok(())
    }

    fn show_text_with_glyph_positioning(
        &mut self,
        elements: &[pdf_operator::TextElement],
    ) -> Result<(), Self::ErrorType> {
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

impl PdfOperatorBackendError for PdfCanvas<'_> {
    type ErrorType = PdfCanvasError;
}
