//! Dispatch of parsed content-stream operators to their canvas handlers.
use pdf_content_stream_operators::variants::PdfOperatorVariant;
use pdf_graphics::{PaintMode, PathFillType};

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Applies one content-stream operator to the canvas.
    ///
    /// Operators without a visual effect on this canvas (compatibility sections, Type 3 glyph
    /// metrics, rendering intent and flatness) are accepted and ignored. Type 3 glyph procedures
    /// filter out `d0`/`d1` before dispatch and apply the uncolored-glyph rule of `d1` there.
    pub(crate) fn apply_operator(&mut self, op: &PdfOperatorVariant) -> Result<(), PdfCanvasError> {
        match op {
            PdfOperatorVariant::LineTo { x, y } => {
                self.current_path.line_to(*x, *y);
                Ok(())
            }
            PdfOperatorVariant::MoveTo { x, y } => {
                self.current_path.move_to(*x, *y);
                Ok(())
            }
            PdfOperatorVariant::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => {
                self.current_path.curve_to(*x1, *y1, *x2, *y2, *x3, *y3);
                Ok(())
            }
            PdfOperatorVariant::CurveToV { x2, y2, x3, y3 } => {
                // `v` uses the current point as its first control point.
                let (x1, y1) = self
                    .current_path
                    .current_point()
                    .ok_or(PdfCanvasError::CurrentPointRequired)?;
                self.current_path.curve_to(x1, y1, *x2, *y2, *x3, *y3);
                Ok(())
            }
            PdfOperatorVariant::CurveToY { x1, y1, x3, y3 } => {
                // `y` uses its end point as its second control point.
                self.current_path.curve_to(*x1, *y1, *x3, *y3, *x3, *y3);
                Ok(())
            }
            PdfOperatorVariant::ClosePath => {
                self.current_path.close();
                Ok(())
            }
            PdfOperatorVariant::Rectangle {
                x,
                y,
                width,
                height,
            } => {
                self.current_path.rect(*x, *y, *width, *height);
                Ok(())
            }
            PdfOperatorVariant::StrokePath => {
                self.end_path(Some((PaintMode::Stroke, PathFillType::default())))
            }
            PdfOperatorVariant::CloseStrokePath => {
                self.current_path.close();
                self.end_path(Some((PaintMode::Stroke, PathFillType::default())))
            }
            PdfOperatorVariant::FillPathNonZero => {
                self.end_path(Some((PaintMode::Fill, PathFillType::Winding)))
            }
            PdfOperatorVariant::FillPathEvenOdd => {
                self.end_path(Some((PaintMode::Fill, PathFillType::EvenOdd)))
            }
            PdfOperatorVariant::FillAndStrokePathNonZero => {
                self.end_path(Some((PaintMode::FillAndStroke, PathFillType::Winding)))
            }
            PdfOperatorVariant::FillAndStrokePathEvenOdd => {
                self.end_path(Some((PaintMode::FillAndStroke, PathFillType::EvenOdd)))
            }
            PdfOperatorVariant::CloseFillAndStrokePathNonZero => {
                self.current_path.close();
                self.end_path(Some((PaintMode::FillAndStroke, PathFillType::Winding)))
            }
            PdfOperatorVariant::CloseFillAndStrokePathEvenOdd => {
                self.current_path.close();
                self.end_path(Some((PaintMode::FillAndStroke, PathFillType::EvenOdd)))
            }
            PdfOperatorVariant::EndPath => self.end_path(None),
            PdfOperatorVariant::ClipNonZero => {
                self.pending_clip = Some(PathFillType::Winding);
                Ok(())
            }
            PdfOperatorVariant::ClipEvenOdd => {
                self.pending_clip = Some(PathFillType::EvenOdd);
                Ok(())
            }
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
            PdfOperatorVariant::ShowText { text } => {
                self.render_text_items(std::slice::from_ref(text))
            }
            PdfOperatorVariant::MoveNextLineShowText { text } => {
                self.move_to_start_of_next_line()?;
                self.render_text_items(std::slice::from_ref(text))
            }
            PdfOperatorVariant::SetSpacingMoveShowText {
                word_spacing,
                char_spacing,
                text,
            } => {
                self.set_word_spacing(*word_spacing)?;
                self.set_character_spacing(*char_spacing)?;
                self.move_to_start_of_next_line()?;
                self.render_text_items(std::slice::from_ref(text))
            }
            PdfOperatorVariant::ShowTextArray { elements } => self.render_text_items(elements),
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
            PdfOperatorVariant::SetStrokeColorSpace { name } => self.set_color_space(name, true),
            PdfOperatorVariant::SetNonStrokingColorSpace { name } => {
                self.set_color_space(name, false)
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
