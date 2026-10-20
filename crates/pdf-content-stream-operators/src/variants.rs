use std::sync::Arc;

use pdf_image::InlineImage;
use pdf_object_reader::ReadResult;

use crate::{
    OperandContext,
    color_operators::*,
    graphics_state_operators::*,
    marked_content_operators::*,
    path_operators::*,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    shadings_operators::PaintShading,
    text_positioning_operators::*,
    text_showing_operators::*,
    text_state_operators::*,
    type3_font_operators::{SetCharWidth, SetCharWidthAndBoundingBox},
    xobject_and_image_operators::*,
};

/// One PDF content-stream operation with its decoded operands.
#[derive(Debug, Clone)]
pub enum PdfOperatorVariant {
    LineTo(LineTo),
    MoveTo(MoveTo),
    CurveTo(CurveTo),
    CurveToV(CurveToV),
    CurveToY(CurveToY),
    /// Closes the current subpath by appending a straight line segment from the current point
    /// to the starting point of the subpath. (PDF operator `h`)
    ClosePath,
    Rectangle(Rectangle),
    /// Strokes the current path.
    StrokePath,
    /// Closes the current subpath and then strokes the path.
    /// This is equivalent to a `ClosePath` followed by a `StrokePath`.
    CloseStrokePath,
    /// Fills the current path using the non-zero winding number rule.
    FillPathNonZero,
    /// Fills the current path using the even-odd rule.
    FillPathEvenOdd,
    /// Fills and then strokes the current path, using the non-zero winding number rule
    /// to determine the region to fill.
    FillAndStrokePathNonZero,
    /// Fills and then strokes the current path, using the even-odd rule to determine the
    /// region to fill.
    FillAndStrokePathEvenOdd,
    /// Closes, fills, and then strokes the current path, using the non-zero winding number
    /// rule to determine the region to fill.
    CloseFillAndStrokePathNonZero,
    /// Closes, fills, and then strokes the current path, using the even-odd rule to determine
    /// the region to fill.
    CloseFillAndStrokePathEvenOdd,
    /// Ends the current path object without filling or stroking it.
    /// This operator is a path-painting no-op, used to discard the current path.
    EndPath,
    /// Modifies the current clipping path by intersecting it with the current path, using the non-zero winding number rule to determine the region to clip.
    ClipNonZero,
    /// Modifies the current clipping path by intersecting it with the current path, using the even-odd rule to determine the region to clip.
    ClipEvenOdd,
    SetGrayFill(SetGrayFill),
    SetGrayStroke(SetGrayStroke),
    SetRGBFill(SetRGBFill),
    SetRGBStroke(SetRGBStroke),
    SetCMYKFill(SetCMYKFill),
    SetCMYKStroke(SetCMYKStroke),
    SetLineWidth(SetLineWidth),
    SetLineCapStyle(SetLineCapStyle),
    SetLineJoinStyle(SetLineJoinStyle),
    SetMiterLimit(SetMiterLimit),
    SetDashPattern(SetDashPattern),
    SetFlatnessTolerance(SetFlatnessTolerance),
    SetGraphicsStateFromDict(SetGraphicsStateFromDict),
    /// Saves the current graphics state on the graphics state stack.
    SaveGraphicsState,
    /// Restores the graphics state by removing the most recently saved state from the stack.
    RestoreGraphicsState,
    ConcatMatrix(ConcatMatrix),
    BeginMarkedContent(BeginMarkedContent),
    BeginMarkedContentWithProps(BeginMarkedContentWithProps),
    /// Ends a marked-content sequence begun by a `BMC` or `BDC` operator.
    EndMarkedContent,
    /// Begins a text object, initializing the text matrix and text line matrix to
    /// the identity matrix.
    BeginText,
    /// Ends a text object, discarding the text matrix and text line matrix.
    EndText,
    MoveTextPosition(MoveTextPosition),
    MoveTextPositionAndSetLeading(MoveTextPositionAndSetLeading),
    SetTextMatrix(SetTextMatrix),
    /// Moves to the start of the next line.
    /// This has the same effect as `MoveTextPosition { tx: 0.0, ty: -leading }`,
    /// where `leading` is the current value of the text leading parameter in the text state.
    MoveToNextLine,
    ShowText(ShowText),
    MoveNextLineShowText(MoveNextLineShowText),
    SetSpacingMoveShowText(SetSpacingMoveShowText),
    ShowTextArray(ShowTextArray),
    SetCharacterSpacing(SetCharacterSpacing),
    SetWordSpacing(SetWordSpacing),
    SetHorizontalScaling(SetHorizontalScaling),
    SetLeading(SetLeading),
    SetFont(SetFont),
    SetRenderingMode(SetRenderingMode),
    SetTextRise(SetTextRise),
    InvokeXObject(InvokeXObject),
    /// Begins a compatibility section (BX).
    ///
    /// According to the PDF specification, a compatibility section allows
    /// consumers to ignore any operators they do not recognize until the
    /// matching EX operator is encountered. For our purposes, we parse and
    /// expose these operators so unknown operators within can be tolerated
    /// by higher-level logic. Backends may choose to track nesting depth,
    /// but by default these operators are no-ops.
    BeginCompatibility,
    InlineImage(Arc<InlineImage>),
    /// Ends a compatibility section (EX).
    EndCompatibility,
    PaintShading(PaintShading),
    SetCharWidthAndBoundingBox(SetCharWidthAndBoundingBox),
    SetCharWidth(SetCharWidth),
    SetRenderingIntent(SetRenderingIntent),
    SetStrokeColorSpace(SetStrokeColorSpace),
    SetNonStrokingColorSpace(SetNonStrokingColorSpace),
    SetStrokingColor(SetStrokingColor),
    SetNonStrokingColor(SetNonStrokingColor),
}

impl PdfOperatorVariant {
    /// Reads the operator named `name` from the operands collected before it.
    ///
    /// Returns `Ok(None)` for an unknown operator or one given the wrong number
    /// of operands. Inline images (`BI`) carry their own grammar and are parsed
    /// from the content stream instead.
    pub fn read(name: &[u8], operands: &mut OperandContext<'_, '_>) -> ReadResult<Option<Self>> {
        let operator = match (name, operands.array().len()) {
            (b"\"", 3) => Self::SetSpacingMoveShowText(SetSpacingMoveShowText::read(operands)?),
            (b"'", 1) => Self::MoveNextLineShowText(MoveNextLineShowText::read(operands)?),
            (b"B", 0) => Self::FillAndStrokePathNonZero,
            (b"B*", 0) => Self::FillAndStrokePathEvenOdd,
            (b"BDC", 2) => {
                Self::BeginMarkedContentWithProps(BeginMarkedContentWithProps::read(operands)?)
            }
            (b"BMC", 1) => Self::BeginMarkedContent(BeginMarkedContent::read(operands)?),
            (b"BT", 0) => Self::BeginText,
            (b"BX", 0) => Self::BeginCompatibility,
            (b"CS", 1) => Self::SetStrokeColorSpace(SetStrokeColorSpace::read(operands)?),
            (b"Do", 1) => Self::InvokeXObject(InvokeXObject::read(operands)?),
            (b"EMC", 0) => Self::EndMarkedContent,
            (b"ET", 0) => Self::EndText,
            (b"EX", 0) => Self::EndCompatibility,
            (b"G", 1) => Self::SetGrayStroke(SetGrayStroke::read(operands)?),
            (b"J", 1) => Self::SetLineCapStyle(SetLineCapStyle::read(operands)?),
            (b"K", 4) => Self::SetCMYKStroke(SetCMYKStroke::read(operands)?),
            (b"M", 1) => Self::SetMiterLimit(SetMiterLimit::read(operands)?),
            (b"Q", 0) => Self::RestoreGraphicsState,
            (b"RG", 3) => Self::SetRGBStroke(SetRGBStroke::read(operands)?),
            (b"S", 0) => Self::StrokePath,
            (b"SC", _) => Self::SetStrokingColor(SetStrokingColor::read_without_pattern(operands)?),
            (b"SCN", _) => Self::SetStrokingColor(SetStrokingColor::read(operands)?),
            (b"T*", 0) => Self::MoveToNextLine,
            (b"TD", 2) => {
                Self::MoveTextPositionAndSetLeading(MoveTextPositionAndSetLeading::read(operands)?)
            }
            (b"TJ", 1) => Self::ShowTextArray(ShowTextArray::read(operands)?),
            (b"TL", 1) => Self::SetLeading(SetLeading::read(operands)?),
            (b"Tc", 1) => Self::SetCharacterSpacing(SetCharacterSpacing::read(operands)?),
            (b"Td", 2) => Self::MoveTextPosition(MoveTextPosition::read(operands)?),
            (b"Tf", 2) => Self::SetFont(SetFont::read(operands)?),
            (b"Tj", 1) => Self::ShowText(ShowText::read(operands)?),
            (b"Tm", 6) => Self::SetTextMatrix(SetTextMatrix::read(operands)?),
            (b"Tr", 1) => Self::SetRenderingMode(SetRenderingMode::read(operands)?),
            (b"Ts", 1) => Self::SetTextRise(SetTextRise::read(operands)?),
            (b"Tw", 1) => Self::SetWordSpacing(SetWordSpacing::read(operands)?),
            (b"Tz", 1) => Self::SetHorizontalScaling(SetHorizontalScaling::read(operands)?),
            (b"W", 0) => Self::ClipNonZero,
            (b"W*", 0) => Self::ClipEvenOdd,
            (b"b", 0) => Self::CloseFillAndStrokePathNonZero,
            (b"b*", 0) => Self::CloseFillAndStrokePathEvenOdd,
            (b"c", 6) => Self::CurveTo(CurveTo::read(operands)?),
            (b"cm", 6) => Self::ConcatMatrix(ConcatMatrix::read(operands)?),
            (b"cs", 1) => Self::SetNonStrokingColorSpace(SetNonStrokingColorSpace::read(operands)?),
            (b"d", 2) => Self::SetDashPattern(SetDashPattern::read(operands)?),
            (b"d0", 2) => Self::SetCharWidth(SetCharWidth::read(operands)?),
            (b"d1", 6) => {
                Self::SetCharWidthAndBoundingBox(SetCharWidthAndBoundingBox::read(operands)?)
            }
            (b"f", 0) => Self::FillPathNonZero,
            (b"f*", 0) => Self::FillPathEvenOdd,
            (b"g", 1) => Self::SetGrayFill(SetGrayFill::read(operands)?),
            (b"gs", 1) => Self::SetGraphicsStateFromDict(SetGraphicsStateFromDict::read(operands)?),
            (b"h", 0) => Self::ClosePath,
            (b"i", 1) => Self::SetFlatnessTolerance(SetFlatnessTolerance::read(operands)?),
            (b"j", 1) => Self::SetLineJoinStyle(SetLineJoinStyle::read(operands)?),
            (b"k", 4) => Self::SetCMYKFill(SetCMYKFill::read(operands)?),
            (b"l", 2) => Self::LineTo(LineTo::read(operands)?),
            (b"m", 2) => Self::MoveTo(MoveTo::read(operands)?),
            (b"n", 0) => Self::EndPath,
            (b"q", 0) => Self::SaveGraphicsState,
            (b"re", 4) => Self::Rectangle(Rectangle::read(operands)?),
            (b"rg", 3) => Self::SetRGBFill(SetRGBFill::read(operands)?),
            (b"ri", 1) => Self::SetRenderingIntent(SetRenderingIntent::read(operands)?),
            (b"s", 0) => Self::CloseStrokePath,
            (b"sc", _) => {
                Self::SetNonStrokingColor(SetNonStrokingColor::read_without_pattern(operands)?)
            }
            (b"scn", _) => Self::SetNonStrokingColor(SetNonStrokingColor::read(operands)?),
            (b"sh", 1) => Self::PaintShading(PaintShading::read(operands)?),
            (b"v", 4) => Self::CurveToV(CurveToV::read(operands)?),
            (b"w", 1) => Self::SetLineWidth(SetLineWidth::read(operands)?),
            (b"y", 4) => Self::CurveToY(CurveToY::read(operands)?),
            _ => return Ok(None),
        };
        Ok(Some(operator))
    }

    /// Dispatches the operation to `backend`.
    pub fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
        match self {
            Self::LineTo(op) => op.call(backend),
            Self::MoveTo(op) => op.call(backend),
            Self::CurveTo(op) => op.call(backend),
            Self::CurveToV(op) => op.call(backend),
            Self::CurveToY(op) => op.call(backend),
            Self::ClosePath => backend.close_path(),
            Self::Rectangle(op) => op.call(backend),
            Self::StrokePath => backend.stroke_path(),
            Self::CloseStrokePath => backend.close_and_stroke_path(),
            Self::FillPathNonZero => backend.fill_path_nonzero_winding(),
            Self::FillPathEvenOdd => backend.fill_path_even_odd(),
            Self::FillAndStrokePathNonZero => backend.fill_and_stroke_path_nonzero_winding(),
            Self::FillAndStrokePathEvenOdd => backend.fill_and_stroke_path_even_odd(),
            Self::CloseFillAndStrokePathNonZero => {
                backend.close_fill_and_stroke_path_nonzero_winding()
            }
            Self::CloseFillAndStrokePathEvenOdd => backend.close_fill_and_stroke_path_even_odd(),
            Self::EndPath => backend.end_path_no_op(),
            Self::ClipNonZero => backend.clip_path_nonzero_winding(),
            Self::ClipEvenOdd => backend.clip_path_even_odd(),
            Self::SetGrayFill(op) => op.call(backend),
            Self::SetGrayStroke(op) => op.call(backend),
            Self::SetRGBFill(op) => op.call(backend),
            Self::SetRGBStroke(op) => op.call(backend),
            Self::SetCMYKFill(op) => op.call(backend),
            Self::SetCMYKStroke(op) => op.call(backend),
            Self::SetLineWidth(op) => op.call(backend),
            Self::SetLineCapStyle(op) => op.call(backend),
            Self::SetLineJoinStyle(op) => op.call(backend),
            Self::SetMiterLimit(op) => op.call(backend),
            Self::SetDashPattern(op) => op.call(backend),
            Self::SetFlatnessTolerance(op) => op.call(backend),
            Self::SetGraphicsStateFromDict(op) => op.call(backend),
            Self::SaveGraphicsState => backend.save_graphics_state(),
            Self::RestoreGraphicsState => backend.restore_graphics_state(),
            Self::ConcatMatrix(op) => op.call(backend),
            Self::BeginMarkedContent(op) => op.call(backend),
            Self::BeginMarkedContentWithProps(op) => op.call(backend),
            Self::EndMarkedContent => backend.end_marked_content(),
            Self::BeginText => backend.begin_text_object(),
            Self::EndText => backend.end_text_object(),
            Self::MoveTextPosition(op) => op.call(backend),
            Self::MoveTextPositionAndSetLeading(op) => op.call(backend),
            Self::SetTextMatrix(op) => op.call(backend),
            Self::MoveToNextLine => backend.move_to_start_of_next_line(),
            Self::ShowText(op) => op.call(backend),
            Self::MoveNextLineShowText(op) => op.call(backend),
            Self::SetSpacingMoveShowText(op) => op.call(backend),
            Self::ShowTextArray(op) => op.call(backend),
            Self::SetCharacterSpacing(op) => op.call(backend),
            Self::SetWordSpacing(op) => op.call(backend),
            Self::SetHorizontalScaling(op) => op.call(backend),
            Self::SetLeading(op) => op.call(backend),
            Self::SetFont(op) => op.call(backend),
            Self::SetRenderingMode(op) => op.call(backend),
            Self::SetTextRise(op) => op.call(backend),
            Self::InvokeXObject(op) => op.call(backend),
            Self::BeginCompatibility => Ok(()),
            Self::InlineImage(image) => backend.paint_inline_image(image),
            Self::EndCompatibility => Ok(()),
            Self::PaintShading(op) => op.call(backend),
            Self::SetCharWidthAndBoundingBox(op) => op.call(backend),
            Self::SetCharWidth(op) => op.call(backend),
            Self::SetRenderingIntent(op) => op.call(backend),
            Self::SetStrokeColorSpace(op) => op.call(backend),
            Self::SetNonStrokingColorSpace(op) => op.call(backend),
            Self::SetStrokingColor(op) => op.call(backend),
            Self::SetNonStrokingColor(op) => op.call(backend),
        }
    }
}
