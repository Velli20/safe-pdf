use crate::TextElement;
use crate::pdf_operator_backend::*;

/// Represents a recorded operation with its parameters.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordedOperation {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    CurveTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    },
    CurveToV {
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    },
    CurveToY {
        x1: f32,
        y1: f32,
        x3: f32,
        y3: f32,
    },
    ClosePath,
    Rectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    StrokePath,
    CloseAndStrokePath,
    FillPathNonZeroWinding,
    FillPathEvenOdd,
    FillAndStrokePathNonZeroWinding,
    FillAndStrokePathEvenOdd,
    CloseFillAndStrokePathNonZeroWinding,
    CloseFillAndStrokePathEvenOdd,
    EndPathNoOp,
    ClipPathNonZeroWinding,
    ClipPathEvenOdd,
    SaveGraphicsState,
    RestoreGraphicsState,
    ConcatMatrix {
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
    },
    SetLineWidth {
        width: f32,
    },
    SetLineCap {
        cap_style: i32,
    },
    SetLineJoin {
        join_style: i32,
    },
    SetMiterLimit {
        limit: f32,
    },
    SetDashPattern {
        dash_array: Vec<f32>,
        dash_phase: f32,
    },
    SetRenderingIntent {
        intent: String,
    },
    SetFlatnessTolerance {
        tolerance: f32,
    },
    SetGraphicsStateFromDict {
        dict_name: String,
    },
    SetStrokingColorSpace {
        name: String,
    },
    SetNonStrokingColorSpace {
        name: String,
    },
    SetStrokingColor {
        components: Vec<f32>,
    },
    SetStrokingColorExtended {
        components: Vec<f32>,
        pattern_name: Option<String>,
    },
    SetNonStrokingColor {
        components: Vec<f32>,
    },
    SetNonStrokingColorExtended {
        components: Vec<f32>,
        pattern_name: Option<String>,
    },
    SetStrokingGray {
        gray: f32,
    },
    SetNonStrokingGray {
        gray: f32,
    },
    SetStrokingRgb {
        r: f32,
        g: f32,
        b: f32,
    },
    SetNonStrokingRgb {
        r: f32,
        g: f32,
        b: f32,
    },
    SetStrokingCmyk {
        c: f32,
        m: f32,
        y: f32,
        k: f32,
    },
    SetNonStrokingCmyk {
        c: f32,
        m: f32,
        y: f32,
        k: f32,
    },
    BeginTextObject,
    EndTextObject,
    SetCharacterSpacing {
        spacing: f32,
    },
    SetWordSpacing {
        spacing: f32,
    },
    SetHorizontalTextScaling {
        scale_percent: f32,
    },
    SetTextLeading {
        leading: f32,
    },
    SetFontAndSize {
        font_name: String,
        size: f32,
    },
    SetTextRenderingMode {
        mode: i32,
    },
    SetTextRise {
        rise: f32,
    },
    MoveTextPosition {
        tx: f32,
        ty: f32,
    },
    MoveTextPositionAndSetLeading {
        tx: f32,
        ty: f32,
    },
    SetTextMatrix {
        a: f32,
        b: f32,
        c: f32,
        d: f32,
        e: f32,
        f: f32,
    },
    MoveToStartOfNextLine,
    ShowText {
        text: Vec<u8>,
    },
    ShowTextWithGlyphPositioning {
        elements: Vec<TextElement>,
    },
    MoveToNextLineAndShowText {
        text: Vec<u8>,
    },
    SetSpacingAndShowText {
        word_spacing: f32,
        char_spacing: f32,
        text: Vec<u8>,
    },
    InvokeXObject {
        xobject_name: String,
    },
    PaintShading {
        shading_name: String,
    },
    MarkPoint {
        tag: String,
    },
    MarkPointWithProperties {
        tag: String,
        properties_name_or_dict: String,
    },
    BeginMarkedContent {
        tag: String,
    },
    BeginMarkedContentWithProperties {
        tag: String,
        properties_name_or_dict: String,
    },
    EndMarkedContent,
}

/// A [`PdfOperatorBackend`] implementation that records all operations called on it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RecordingBackend {
    pub operations: Vec<RecordedOperation>,
}

impl RecordingBackend {
    pub fn new() -> Self {
        Default::default()
    }
}

impl PdfOperatorBackendError for RecordingBackend {
    type ErrorType = ();
}

impl PathConstructionOps for RecordingBackend {
    fn move_to(&mut self, x: f32, y: f32) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::MoveTo { x, y });
        Ok(())
    }

    fn line_to(&mut self, x: f32, y: f32) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::LineTo { x, y });
        Ok(())
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
        self.operations.push(RecordedOperation::CurveTo {
            x1,
            y1,
            x2,
            y2,
            x3,
            y3,
        });
        Ok(())
    }

    fn curve_to_v(&mut self, x2: f32, y2: f32, x3: f32, y3: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::CurveToV { x2, y2, x3, y3 });
        Ok(())
    }

    fn curve_to_y(&mut self, x1: f32, y1: f32, x3: f32, y3: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::CurveToY { x1, y1, x3, y3 });
        Ok(())
    }

    fn close_path(&mut self) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::ClosePath);
        Ok(())
    }

    fn rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::Rectangle {
            x,
            y,
            width,
            height,
        });
        Ok(())
    }
}

impl PathPaintingOps for RecordingBackend {
    fn stroke_path(&mut self) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::StrokePath);
        Ok(())
    }

    fn close_and_stroke_path(&mut self) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::CloseAndStrokePath);
        Ok(())
    }

    fn fill_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::FillPathNonZeroWinding);
        Ok(())
    }

    fn fill_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::FillPathEvenOdd);
        Ok(())
    }

    fn fill_and_stroke_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::FillAndStrokePathNonZeroWinding);
        Ok(())
    }

    fn fill_and_stroke_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::FillAndStrokePathEvenOdd);
        Ok(())
    }

    fn close_fill_and_stroke_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::CloseFillAndStrokePathNonZeroWinding);
        Ok(())
    }

    fn close_fill_and_stroke_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::CloseFillAndStrokePathEvenOdd);
        Ok(())
    }

    fn end_path_no_op(&mut self) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::EndPathNoOp);
        Ok(())
    }
}

impl ClippingPathOps for RecordingBackend {
    fn clip_path_nonzero_winding(&mut self) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::ClipPathNonZeroWinding);
        Ok(())
    }

    fn clip_path_even_odd(&mut self) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::ClipPathEvenOdd);
        Ok(())
    }
}

impl GraphicsStateOps for RecordingBackend {
    fn save_graphics_state(&mut self) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::SaveGraphicsState);
        Ok(())
    }

    fn restore_graphics_state(&mut self) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::RestoreGraphicsState);
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
        self.operations
            .push(RecordedOperation::ConcatMatrix { a, b, c, d, e, f });
        Ok(())
    }

    fn set_line_width(&mut self, width: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetLineWidth { width });
        Ok(())
    }

    fn set_line_cap(&mut self, cap_style: i32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetLineCap { cap_style });
        Ok(())
    }

    fn set_line_join(&mut self, join_style: i32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetLineJoin { join_style });
        Ok(())
    }

    fn set_miter_limit(&mut self, limit: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetMiterLimit { limit });
        Ok(())
    }

    fn set_dash_pattern(
        &mut self,
        dash_array: &[f32],
        dash_phase: f32,
    ) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::SetDashPattern {
            dash_array: dash_array.to_vec(),
            dash_phase,
        });
        Ok(())
    }

    fn set_rendering_intent(&mut self, intent: &str) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::SetRenderingIntent {
            intent: intent.to_string(),
        });
        Ok(())
    }

    fn set_flatness_tolerance(&mut self, tolerance: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetFlatnessTolerance { tolerance });
        Ok(())
    }

    fn set_graphics_state_from_dict(&mut self, dict_name: &str) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetGraphicsStateFromDict {
                dict_name: dict_name.to_string(),
            });
        Ok(())
    }
}

impl ColorOps for RecordingBackend {
    fn set_stroking_color_space(&mut self, name: &str) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetStrokingColorSpace {
                name: name.to_string(),
            });
        Ok(())
    }

    fn set_non_stroking_color_space(&mut self, name: &str) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetNonStrokingColorSpace {
                name: name.to_string(),
            });
        Ok(())
    }

    fn set_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::SetStrokingColor {
            components: components.to_vec(),
        });
        Ok(())
    }

    fn set_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: Option<&str>,
    ) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetStrokingColorExtended {
                components: components.to_vec(),
                pattern_name: pattern_name.map(|s| s.to_string()),
            });
        Ok(())
    }

    fn set_non_stroking_color(&mut self, components: &[f32]) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetNonStrokingColor {
                components: components.to_vec(),
            });
        Ok(())
    }

    fn set_non_stroking_color_extended(
        &mut self,
        components: &[f32],
        pattern_name: Option<&str>,
    ) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetNonStrokingColorExtended {
                components: components.to_vec(),
                pattern_name: pattern_name.map(|s| s.to_string()),
            });
        Ok(())
    }

    fn set_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetStrokingGray { gray });
        Ok(())
    }

    fn set_non_stroking_gray(&mut self, gray: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetNonStrokingGray { gray });
        Ok(())
    }

    fn set_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetStrokingRgb { r, g, b });
        Ok(())
    }

    fn set_non_stroking_rgb(&mut self, r: f32, g: f32, b: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetNonStrokingRgb { r, g, b });
        Ok(())
    }

    fn set_stroking_cmyk(&mut self, c: f32, m: f32, y: f32, k: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetStrokingCmyk { c, m, y, k });
        Ok(())
    }

    fn set_non_stroking_cmyk(
        &mut self,
        c: f32,
        m: f32,
        y: f32,
        k: f32,
    ) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetNonStrokingCmyk { c, m, y, k });
        Ok(())
    }
}

impl TextObjectOps for RecordingBackend {
    fn begin_text_object(&mut self) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::BeginTextObject);
        Ok(())
    }

    fn end_text_object(&mut self) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::EndTextObject);
        Ok(())
    }
}

impl TextStateOps for RecordingBackend {
    fn set_character_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetCharacterSpacing { spacing });
        Ok(())
    }

    fn set_word_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetWordSpacing { spacing });
        Ok(())
    }

    fn set_horizontal_text_scaling(&mut self, scale_percent: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetHorizontalTextScaling { scale_percent });
        Ok(())
    }

    fn set_text_leading(&mut self, leading: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetTextLeading { leading });
        Ok(())
    }

    fn set_font_and_size(&mut self, font_name: &str, size: f32) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::SetFontAndSize {
            font_name: font_name.to_string(),
            size,
        });
        Ok(())
    }

    fn set_text_rendering_mode(&mut self, mode: i32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetTextRenderingMode { mode });
        Ok(())
    }

    fn set_text_rise(&mut self, rise: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetTextRise { rise });
        Ok(())
    }
}

impl TextPositioningOps for RecordingBackend {
    fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::MoveTextPosition { tx, ty });
        Ok(())
    }

    fn move_text_position_and_set_leading(
        &mut self,
        tx: f32,
        ty: f32,
    ) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::MoveTextPositionAndSetLeading { tx, ty });
        Ok(())
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
        self.operations
            .push(RecordedOperation::SetTextMatrix { a, b, c, d, e, f });
        Ok(())
    }

    fn move_to_start_of_next_line(&mut self) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::MoveToStartOfNextLine);
        Ok(())
    }
}

impl TextShowingOps for RecordingBackend {
    fn show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::ShowText {
            text: text.to_vec(),
        });
        Ok(())
    }

    fn show_text_with_glyph_positioning(
        &mut self,
        elements: &[TextElement],
    ) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::ShowTextWithGlyphPositioning {
                elements: elements.to_vec(),
            });
        Ok(())
    }

    fn move_to_next_line_and_show_text(&mut self, text: &[u8]) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::MoveToNextLineAndShowText {
                text: text.to_vec(),
            });
        Ok(())
    }

    fn set_spacing_and_show_text(
        &mut self,
        word_spacing: f32,
        char_spacing: f32,
        text: &[u8],
    ) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::SetSpacingAndShowText {
                word_spacing,
                char_spacing,
                text: text.to_vec(),
            });
        Ok(())
    }
}

impl XObjectOps for RecordingBackend {
    fn invoke_xobject(&mut self, xobject_name: &str) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::InvokeXObject {
            xobject_name: xobject_name.to_string(),
        });
        Ok(())
    }
}

impl ShadingOps for RecordingBackend {
    fn paint_shading(&mut self, shading_name: &str) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::PaintShading {
            shading_name: shading_name.to_string(),
        });
        Ok(())
    }
}

impl MarkedContentOps for RecordingBackend {
    fn mark_point(&mut self, tag: &str) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::MarkPoint {
            tag: tag.to_string(),
        });
        Ok(())
    }

    fn mark_point_with_properties(
        &mut self,
        tag: &str,
        properties_name_or_dict: &str,
    ) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::MarkPointWithProperties {
                tag: tag.to_string(),
                properties_name_or_dict: properties_name_or_dict.to_string(),
            });
        Ok(())
    }

    fn begin_marked_content(&mut self, tag: &str) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::BeginMarkedContent {
            tag: tag.to_string(),
        });
        Ok(())
    }

    fn begin_marked_content_with_properties(
        &mut self,
        tag: &str,
        properties_name_or_dict: &str,
    ) -> Result<(), Self::ErrorType> {
        self.operations
            .push(RecordedOperation::BeginMarkedContentWithProperties {
                tag: tag.to_string(),
                properties_name_or_dict: properties_name_or_dict.to_string(),
            });
        Ok(())
    }

    fn end_marked_content(&mut self) -> Result<(), Self::ErrorType> {
        self.operations.push(RecordedOperation::EndMarkedContent);
        Ok(())
    }
}

impl PdfOperatorBackend for RecordingBackend {}
