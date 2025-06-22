use crate::pdf_canvas::PdfCanvas;
use crate::pdf_path::PdfPath;
use crate::transform::Transform;
use crate::{PathFillType, error::PdfCanvasError};
use pdf_content_stream::pdf_operator_backend::{
    TextObjectOps, TextPositioningOps, TextShowingOps, TextStateOps,
};
use pdf_font::font::FontSubType;
use pdf_object::ObjectVariant;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

impl<'a> TextPositioningOps for PdfCanvas<'a> {
    fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), Self::ErrorType> {
        let mat = Transform::from_translate(tx, ty);
        self.current_state_mut()?
            .text_state
            .line_matrix
            .concat(&mat);
        self.current_state_mut()?.text_state.matrix =
            self.current_state()?.text_state.line_matrix.clone();
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
        self.current_state_mut()?.text_state.line_matrix = mat.clone();
        self.current_state_mut()?.text_state.matrix = mat;
        Ok(())
    }

    fn move_to_start_of_next_line(&mut self) -> Result<(), Self::ErrorType> {
        todo!("Implement T* operator")
    }
}

impl<'a> TextObjectOps for PdfCanvas<'a> {
    fn begin_text_object(&mut self) -> Result<(), Self::ErrorType> {
        Ok(())
    }

    fn end_text_object(&mut self) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.matrix = Transform::identity();
        self.current_state_mut()?.text_state.line_matrix = Transform::identity();
        Ok(())
    }
}

impl<'a> TextStateOps for PdfCanvas<'a> {
    fn set_character_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.character_spacing = spacing;
        Ok(())
    }

    fn set_word_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.word_spacing = spacing;
        Ok(())
    }

    fn set_horizontal_text_scaling(&mut self, scale_percent: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.horizontal_scaling = scale_percent;
        Ok(())
    }

    fn set_text_leading(&mut self, leading: f32) -> Result<(), Self::ErrorType> {
        todo!("Implement text leading TL: {}", leading)
    }

    fn set_font_and_size(&mut self, font_name: &str, size: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.font_size = size;

        let resources = self
            .page
            .resources
            .as_ref()
            .ok_or(PdfCanvasError::MissingPageResources)?;

        let font = resources
            .fonts
            .get(font_name)
            .ok_or(PdfCanvasError::FontNotFound(font_name.to_string()))?;

        if let Some(cid_font) = &font.cid_font {
            if let Some(font_file) = &cid_font.descriptor.font_file {
                if let ObjectVariant::Stream(s) = &font_file {
                    let face =
                        Face::parse(s.data.as_slice(), 0).expect("Failed to parse font face");
                    self.current_state_mut()?.text_state.font_face = Some(face);
                }
            }
        }
        self.current_state_mut()?.text_state.font = Some(font);
        Ok(())
    }

    fn set_text_rendering_mode(&mut self, mode: i32) -> Result<(), Self::ErrorType> {
        println!("Implement text rendering mode Tr: {}", mode);
        Ok(())
    }

    fn set_text_rise(&mut self, rise: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.rise = rise;
        Ok(())
    }
}

impl<'a> TextShowingOps for PdfCanvas<'a> {
    fn set_char_width_and_bounding_box(
        &mut self,
        wx: f32,
        wy: f32,
        llx: f32,
        lly: f32,
        urx: f32,
        ury: f32,
    ) -> Result<(), Self::ErrorType> {
        let _ = (wx, wy, llx, lly, urx, ury);
        Ok(())
    }

    fn show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        let text_state = &self.current_state()?.text_state.clone();
        let current_font = text_state.font.ok_or(PdfCanvasError::NoCurrentFont)?;
        if current_font.subtype == FontSubType::Type3 {
            return self.show_type3_font_text(text);
        }
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

        let fill_color = self.current_state()?.fill_color;
        let mut iter = text.iter();
        if current_font.encoding.is_some() {
            let _ = iter.next();
        }

        let cmap = current_font
            .cmap
            .as_ref()
            .ok_or(PdfCanvasError::NoCharacterMapForFont(
                current_font.base_font.clone(),
            ))?;

        // Iterate over each character in the input text.
        while let Some(char_code_byte) = iter.next() {
            if current_font.encoding.is_some() {
                let _ = iter.next();
            }

            let char_code = *char_code_byte;

            let mut glyph_id = GlyphId(char_code as u16);

            // Compose the final transformation matrix for this glyph:
            // m_params -> text matrix -> current transformation matrix
            let mut glyph_matrix_for_char = m_params.clone();
            glyph_matrix_for_char.concat(&self.current_state()?.text_state.matrix);
            glyph_matrix_for_char.concat(&self.current_state()?.transform);

            // Build the glyph outline using the composed transform.
            let mut builder = PdfGlyphOutline::new(glyph_matrix_for_char);

            if let Some(a) = cmap.get_mapping(*char_code_byte as u32) {
                if let Some(x) = face.glyph_index(a) {
                    glyph_id = x;
                }
            }

            face.outline_glyph(glyph_id, &mut builder);

            // Fill it on the canvas
            self.canvas
                .fill_path(&builder.path, PathFillType::Winding, fill_color);

            // Determine the glyph's advance width in font units.
            let w0_glyph_units = current_font
                .cid_font
                .as_ref()
                .unwrap()
                .widths
                .as_ref()
                .and_then(|w_array| w_array.get_width(char_code as i64))
                .unwrap_or_else(|| current_font.cid_font.as_ref().unwrap().default_width as f32);

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
            self.current_state_mut()?
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
        println!("Implement TJ operator: {:?}", elements);
        Ok(())
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
