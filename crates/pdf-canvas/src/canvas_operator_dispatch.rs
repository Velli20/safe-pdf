//! Dispatch of parsed content-stream operators to their canvas handlers.
use pdf_content_stream_operators::variants::PdfOperatorVariant;

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Applies one content-stream operator to the canvas.
    ///
    /// Operators without a visual effect on this canvas (compatibility sections, Type 3 glyph
    /// metrics, rendering intent and flatness) are accepted and ignored.
    pub(crate) fn apply_operator(&mut self, op: &PdfOperatorVariant) -> Result<(), PdfCanvasError> {
        match op {
            PdfOperatorVariant::LineTo { x, y } => self.line_to(*x, *y),
            PdfOperatorVariant::MoveTo { x, y } => self.move_to(*x, *y),
            PdfOperatorVariant::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => self.curve_to(*x1, *y1, *x2, *y2, *x3, *y3),
            PdfOperatorVariant::CurveToV { x2, y2, x3, y3 } => self.curve_to_v(*x2, *y2, *x3, *y3),
            PdfOperatorVariant::CurveToY { x1, y1, x3, y3 } => self.curve_to_y(*x1, *y1, *x3, *y3),
            PdfOperatorVariant::ClosePath => self.close_path(),
            PdfOperatorVariant::Rectangle {
                x,
                y,
                width,
                height,
            } => self.rectangle(*x, *y, *width, *height),
            PdfOperatorVariant::StrokePath => self.stroke_path(),
            PdfOperatorVariant::CloseStrokePath => self.close_and_stroke_path(),
            PdfOperatorVariant::FillPathNonZero => self.fill_path_nonzero_winding(),
            PdfOperatorVariant::FillPathEvenOdd => self.fill_path_even_odd(),
            PdfOperatorVariant::FillAndStrokePathNonZero => {
                self.fill_and_stroke_path_nonzero_winding()
            }
            PdfOperatorVariant::FillAndStrokePathEvenOdd => self.fill_and_stroke_path_even_odd(),
            PdfOperatorVariant::CloseFillAndStrokePathNonZero => {
                self.close_fill_and_stroke_path_nonzero_winding()
            }
            PdfOperatorVariant::CloseFillAndStrokePathEvenOdd => {
                self.close_fill_and_stroke_path_even_odd()
            }
            PdfOperatorVariant::EndPath => self.end_path_no_op(),
            PdfOperatorVariant::ClipNonZero => self.clip_path_nonzero_winding(),
            PdfOperatorVariant::ClipEvenOdd => self.clip_path_even_odd(),
            PdfOperatorVariant::SetGrayFill { gray } => self.set_non_stroking_gray(*gray),
            PdfOperatorVariant::SetGrayStroke { gray } => self.set_stroking_gray(*gray),
            PdfOperatorVariant::SetRGBFill { r, g, b } => self.set_non_stroking_rgb(*r, *g, *b),
            PdfOperatorVariant::SetRGBStroke { r, g, b } => self.set_stroking_rgb(*r, *g, *b),
            PdfOperatorVariant::SetCMYKFill { c, m, y, k } => {
                self.set_non_stroking_cmyk(*c, *m, *y, *k)
            }
            PdfOperatorVariant::SetCMYKStroke { c, m, y, k } => {
                self.set_stroking_cmyk(*c, *m, *y, *k)
            }
            PdfOperatorVariant::SetLineWidth { width } => self.set_line_width(*width),
            PdfOperatorVariant::SetLineCapStyle { style } => self.set_line_cap(*style),
            PdfOperatorVariant::SetLineJoinStyle { style } => self.set_line_join(*style),
            PdfOperatorVariant::SetMiterLimit { limit } => self.set_miter_limit(*limit),
            PdfOperatorVariant::SetDashPattern { array, phase } => {
                self.set_dash_pattern(array, *phase)
            }
            PdfOperatorVariant::SetGraphicsStateFromDict { dict_name } => {
                self.set_graphics_state_from_dict(dict_name)
            }
            PdfOperatorVariant::SaveGraphicsState => self.save(),
            PdfOperatorVariant::RestoreGraphicsState => self.restore(),
            PdfOperatorVariant::ConcatMatrix { matrix } => self.concat_matrix(matrix),
            PdfOperatorVariant::BeginMarkedContent { tag } => self.begin_marked_content(tag),
            PdfOperatorVariant::BeginMarkedContentWithProps(op) => {
                self.begin_marked_content_with_properties(op.tag(), op.properties())
            }
            PdfOperatorVariant::EndMarkedContent => self.end_marked_content(),
            PdfOperatorVariant::BeginText => self.begin_text_object(),
            PdfOperatorVariant::EndText => self.end_text_object(),
            PdfOperatorVariant::MoveTextPosition { tx, ty } => self.move_text_position(*tx, *ty),
            PdfOperatorVariant::MoveTextPositionAndSetLeading { tx, ty } => {
                self.move_text_position_and_set_leading(*tx, *ty)
            }
            PdfOperatorVariant::SetTextMatrix { matrix } => self.set_text_matrix(matrix),
            PdfOperatorVariant::MoveToNextLine => self.move_to_start_of_next_line(),
            PdfOperatorVariant::ShowText { text } => self.show_text(text),
            PdfOperatorVariant::MoveNextLineShowText { text } => {
                self.move_to_next_line_and_show_text(text)
            }
            PdfOperatorVariant::SetSpacingMoveShowText {
                word_spacing,
                char_spacing,
                text,
            } => self.set_spacing_and_show_text(*word_spacing, *char_spacing, text),
            PdfOperatorVariant::ShowTextArray { elements } => {
                self.show_text_with_glyph_positioning(elements)
            }
            PdfOperatorVariant::SetCharacterSpacing { spacing } => {
                self.set_character_spacing(*spacing)
            }
            PdfOperatorVariant::SetWordSpacing { spacing } => self.set_word_spacing(*spacing),
            PdfOperatorVariant::SetHorizontalScaling { scale } => {
                self.set_horizontal_text_scaling(*scale)
            }
            PdfOperatorVariant::SetLeading { leading } => self.set_text_leading(*leading),
            PdfOperatorVariant::SetFont { name, size } => self.set_font_and_size(name, *size),
            PdfOperatorVariant::SetRenderingMode { mode } => self.set_text_rendering_mode(*mode),
            PdfOperatorVariant::SetTextRise { rise } => self.set_text_rise(*rise),
            PdfOperatorVariant::InvokeXObject { name } => self.invoke_xobject(name),
            PdfOperatorVariant::InlineImage(image) => self.paint_inline_image(image),
            PdfOperatorVariant::PaintShading { name } => self.paint_shading(name),
            PdfOperatorVariant::SetStrokeColorSpace { name } => self.set_stroking_color_space(name),
            PdfOperatorVariant::SetNonStrokingColorSpace { name } => {
                self.set_non_stroking_color_space(name)
            }
            PdfOperatorVariant::SetStrokingColor(color) => match &color.pattern {
                Some(pattern) => self.set_stroking_color_extended(&color.components, pattern),
                None => self.set_stroking_color(&color.components),
            },
            PdfOperatorVariant::SetNonStrokingColor(color) => match &color.pattern {
                Some(pattern) => self.set_non_stroking_color_extended(&color.components, pattern),
                None => self.set_non_stroking_color(&color.components),
            },
            PdfOperatorVariant::BeginCompatibility
            | PdfOperatorVariant::EndCompatibility
            | PdfOperatorVariant::SetCharWidthAndBoundingBox { .. }
            | PdfOperatorVariant::SetCharWidth { .. }
            | PdfOperatorVariant::SetRenderingIntent { .. }
            | PdfOperatorVariant::SetFlatnessTolerance { .. } => Ok(()),
        }
    }
}
