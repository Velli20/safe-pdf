use error::PdfCanvasError;
use pdf_font::font::Font;
use pdf_operator::pdf_operator_backend::{
    ClippingPathOps, ColorOps, GraphicsStateOps, MarkedContentOps, PathConstructionOps,
    PathPaintingOps, PdfOperatorBackend, PdfOperatorBackendError, ShadingOps, TextObjectOps,
    TextPositioningOps, TextShowingOps, TextStateOps, XObjectOps,
};
use pdf_page::page::PdfPage;
use pdf_path::PdfPath;

pub mod error;
pub mod pdf_path;

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

pub struct PdfCanvas<'a> {
    current_path: Option<PdfPath>,
    canvas: &'a mut dyn CanvasBackend,
    page: &'a PdfPage,
    current_font: Option<&'a Font>
}


impl<'a> PdfCanvas<'a> {
    pub fn new(backend: &'a mut dyn CanvasBackend, page: &'a PdfPage) -> Self {
        Self {
            current_path: None,
            canvas: backend,
            page,
            current_font: None
        }
    }
}

pub trait CanvasBackend {
    fn draw_path(&mut self, path: &PdfPath, mode: PaintMode, fill_type: PathFillType);

    fn save(&mut self);

    fn restore(&mut self);
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
        self.canvas.save();
        Ok(())
    }

    fn restore_graphics_state(&mut self) -> Result<(), Self::ErrorType> {
        self.canvas.restore();
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
        println!("begin_text_object");
        // text_matrix = Transform::new();
        // text_line_matrix = Transform::new();

        Ok(())
    }

    fn end_text_object(&mut self) -> Result<(), Self::ErrorType> {
        println!("end_text_object");
        Ok(())
    }
}

impl<'a> TextStateOps for PdfCanvas<'a> {
    fn set_character_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_word_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_horizontal_text_scaling(&mut self, scale_percent: f32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_text_leading(&mut self, leading: f32) -> Result<(), Self::ErrorType> {
        todo!()
    }

    fn set_font_and_size(&mut self, font_name: &str, size: f32) -> Result<(), Self::ErrorType> {
        println!("set_font_and_size name: {} size: {}", font_name, size);
        if let Some(resources) = &self.page.resources {
            if let Some(font) = resources.fonts.get(font_name) {
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
        todo!()
    }
}

impl<'a> TextPositioningOps for PdfCanvas<'a> {
    fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), Self::ErrorType> {
        println!("move_text_position tx: {} ty: {}", tx, ty);
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

impl<'a> TextShowingOps for PdfCanvas<'a> {
    fn show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        println!("show_text: {:?}", text);
        if let Some(font) = self.current_font {
            if let Some(cmap) = &font.cmap {
                for c in text {
                    let mappd = cmap.get_mapping(*c as u32);
                    if let Some(mappd) = mappd {
                        println!("mappd: {:?}", mappd as char);
                    }
                }
            }
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

impl<'a> PathConstructionOps for PdfCanvas<'a> {
    fn move_to(&mut self, x: f32, y: f32) -> Result<(), Self::ErrorType> {
        self.current_path
            .get_or_insert(PdfPath::default())
            .move_to(x, y)
    }

    fn line_to(&mut self, x: f32, y: f32) -> Result<(), Self::ErrorType> {
        self.current_path
            .get_or_insert(PdfPath::default())
            .line_to(x, y)
    }

    fn curve_to(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    ) -> Result<(), Self::ErrorType> {
        self.current_path
            .get_or_insert(PdfPath::default())
            .curve_to(x1, y1, x2, y2, x3, y3)
    }

    fn curve_to_v(&mut self, x2: f32, y2: f32, x3: f32, y3: f32) -> Result<(), Self::ErrorType> {
        let path = self.current_path.get_or_insert(PdfPath::default());
        if let Some((x, y)) = path.current_point() {
            path.curve_to(x, y, x2, y2, x3, y3)
        } else {
            Err(PdfCanvasError::NoCurrentPoint)
        }
    }

    fn curve_to_y(&mut self, x1: f32, y1: f32, x3: f32, y3: f32) -> Result<(), Self::ErrorType> {
        self.current_path
            .get_or_insert(PdfPath::default())
            .curve_to(x1, y1, x3, y3, x3, y3)
    }

    fn close_path(&mut self) -> Result<(), Self::ErrorType> {
        self.current_path.get_or_insert(PdfPath::default()).close()
    }

    fn rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Result<(), Self::ErrorType> {
        let path = self.current_path.get_or_insert(PdfPath::default());

        path.move_to(x, y)?;
        path.line_to(x + width, y)?;
        path.line_to(x + width, y + height)?;
        path.line_to(x, y + height)?;
        path.close()
    }
}

impl<'a> PathPaintingOps for PdfCanvas<'a> {
    fn stroke_path(&mut self) -> Result<(), Self::ErrorType> {
        self.paint_taken_path(PaintMode::Stroke, PathFillType::default())
    }

    fn close_and_stroke_path(&mut self) -> Result<(), Self::ErrorType> {
        self.close_path()?;
        self.stroke_path()
    }

    fn fill_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.paint_taken_path(PaintMode::Fill, PathFillType::Winding)
    }

    fn fill_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.paint_taken_path(PaintMode::Fill, PathFillType::EvenOdd)
    }

    fn fill_and_stroke_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.paint_taken_path(PaintMode::FillAndStroke, PathFillType::Winding)
    }

    fn fill_and_stroke_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.paint_taken_path(PaintMode::FillAndStroke, PathFillType::EvenOdd)
    }

    fn close_fill_and_stroke_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.close_path()?;
        self.fill_and_stroke_path_nonzero_winding()
    }

    fn close_fill_and_stroke_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.close_path()?;
        self.fill_and_stroke_path_even_odd()
    }

    fn end_path_no_op(&mut self) -> Result<(), Self::ErrorType> {
        // Discard the current path, making it undefined.
        self.current_path.take();
        Ok(())
    }
}
