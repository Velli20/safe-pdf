use crate::{
    clipping_path_operators::*, color_operators::*, error::PdfPainterError,
    graphics_state_operators::*, marked_content_operators::*, path_operators::*,
    path_paint_operators::*, pdf_operator::PdfOperatorVariant, text_object_operators::*,
    text_positioning_operators::*, text_showing_operators::*, text_state_operators::*,
    xobject_and_image_operators::*,
};

/// Defines a mapping between a PDF operator's string representation (e.g., "m" for MoveTo)
/// and a function that can parse that operator and its operands from a content stream.
/// This is used to dynamically dispatch to the correct parsing logic based on the operator
/// encountered in the PDF content.
pub struct OperationMap {
    /// The string representation of the PDF operator (e.g., "m", "BT", "rg").
    name: &'static str,
    /// A function pointer to the parser for this specific operator.
    /// The parser function takes a mutable vector of `PdfOperator` trait objects,
    /// reads the operator and its operands from an (implicit) input stream,
    /// and adds the newly created operator object to the vector.
    parser: fn() -> Result<PdfOperatorVariant, PdfPainterError>,
}

pub const READ_MAP: &'static [OperationMap] = &[
    OperationMap {
        name: ClipNonZero::operator_name(),
        parser: ClipNonZero::read,
    },
    OperationMap {
        name: ClipEvenOdd::operator_name(),
        parser: ClipEvenOdd::read,
    },
    OperationMap {
        name: SetGrayFill::operator_name(),
        parser: SetGrayFill::read,
    },
    OperationMap {
        name: SetGrayStroke::operator_name(),
        parser: SetGrayStroke::read,
    },
    OperationMap {
        name: SetRGBFill::operator_name(),
        parser: SetRGBFill::read,
    },
    OperationMap {
        name: SetRGBStroke::operator_name(),
        parser: SetRGBStroke::read,
    },
    OperationMap {
        name: SetCMYKFill::operator_name(),
        parser: SetCMYKFill::read,
    },
    OperationMap {
        name: SetCMYKStroke::operator_name(),
        parser: SetCMYKStroke::read,
    },
    OperationMap {
        name: SetLineWidth::operator_name(),
        parser: SetLineWidth::read,
    },
    OperationMap {
        name: SetLineCapStyle::operator_name(),
        parser: SetLineCapStyle::read,
    },
    OperationMap {
        name: SetLineJoinStyle::operator_name(),
        parser: SetLineJoinStyle::read,
    },
    OperationMap {
        name: SetMiterLimit::operator_name(),
        parser: SetMiterLimit::read,
    },
    OperationMap {
        name: SetDashPattern::operator_name(),
        parser: SetDashPattern::read,
    },
    OperationMap {
        name: SaveGraphicsState::operator_name(),
        parser: SaveGraphicsState::read,
    },
    OperationMap {
        name: RestoreGraphicsState::operator_name(),
        parser: RestoreGraphicsState::read,
    },
    OperationMap {
        name: ConcatMatrix::operator_name(),
        parser: ConcatMatrix::read,
    },
    OperationMap {
        name: BeginMarkedContent::operator_name(),
        parser: BeginMarkedContent::read,
    },
    OperationMap {
        name: BeginMarkedContentWithProps::operator_name(),
        parser: BeginMarkedContentWithProps::read,
    },
    OperationMap {
        name: EndMarkedContent::operator_name(),
        parser: EndMarkedContent::read,
    },
    OperationMap {
        name: MoveTo::operator_name(),
        parser: MoveTo::read,
    },
    OperationMap {
        name: LineTo::operator_name(),
        parser: LineTo::read,
    },
    OperationMap {
        name: CurveTo::operator_name(),
        parser: CurveTo::read,
    },
    OperationMap {
        name: CurveToV::operator_name(),
        parser: CurveToV::read,
    },
    OperationMap {
        name: CurveToY::operator_name(),
        parser: CurveToY::read,
    },
    OperationMap {
        name: ClosePath::operator_name(),
        parser: ClosePath::read,
    },
    OperationMap {
        name: Rectangle::operator_name(),
        parser: Rectangle::read,
    },
    OperationMap {
        name: StrokePath::operator_name(),
        parser: StrokePath::read,
    },
    OperationMap {
        name: CloseStrokePath::operator_name(),
        parser: CloseStrokePath::read,
    },
    OperationMap {
        name: FillPathNonZero::operator_name(),
        parser: FillPathNonZero::read,
    },
    OperationMap {
        name: FillPathEvenOdd::operator_name(),
        parser: FillPathEvenOdd::read,
    },
    OperationMap {
        name: FillAndStrokePathNonZero::operator_name(),
        parser: FillAndStrokePathNonZero::read,
    },
    OperationMap {
        name: FillAndStrokePathEvenOdd::operator_name(),
        parser: FillAndStrokePathEvenOdd::read,
    },
    OperationMap {
        name: CloseFillAndStrokePathNonZero::operator_name(),
        parser: CloseFillAndStrokePathNonZero::read,
    },
    OperationMap {
        name: CloseFillAndStrokePathEvenOdd::operator_name(),
        parser: CloseFillAndStrokePathEvenOdd::read,
    },
    OperationMap {
        name: EndPath::operator_name(),
        parser: EndPath::read,
    },
    OperationMap {
        name: BeginText::operator_name(),
        parser: BeginText::read,
    },
    OperationMap {
        name: EndText::operator_name(),
        parser: EndText::read,
    },
    OperationMap {
        name: MoveTextPosition::operator_name(),
        parser: MoveTextPosition::read,
    },
    OperationMap {
        name: MoveTextPositionAndSetLeading::operator_name(),
        parser: MoveTextPositionAndSetLeading::read,
    },
    OperationMap {
        name: SetTextMatrix::operator_name(),
        parser: SetTextMatrix::read,
    },
    OperationMap {
        name: MoveToNextLine::operator_name(),
        parser: MoveToNextLine::read,
    },
    OperationMap {
        name: ShowText::operator_name(),
        parser: ShowText::read,
    },
    OperationMap {
        name: MoveNextLineShowText::operator_name(),
        parser: MoveNextLineShowText::read,
    },
    OperationMap {
        name: SetSpacingMoveShowText::operator_name(),
        parser: SetSpacingMoveShowText::read,
    },
    OperationMap {
        name: ShowTextArray::operator_name(),
        parser: ShowTextArray::read,
    },
    OperationMap {
        name: SetCharacterSpacing::operator_name(),
        parser: SetCharacterSpacing::read,
    },
    OperationMap {
        name: SetWordSpacing::operator_name(),
        parser: SetWordSpacing::read,
    },
    OperationMap {
        name: SetHorizontalScaling::operator_name(),
        parser: SetHorizontalScaling::read,
    },
    OperationMap {
        name: SetLeading::operator_name(),
        parser: SetLeading::read,
    },
    OperationMap {
        name: SetFont::operator_name(),
        parser: SetFont::read,
    },
    OperationMap {
        name: SetRenderingMode::operator_name(),
        parser: SetRenderingMode::read,
    },
    OperationMap {
        name: SetTextRise::operator_name(),
        parser: SetTextRise::read,
    },
    OperationMap {
        name: InvokeXObject::operator_name(),
        parser: InvokeXObject::read,
    },
    OperationMap {
        name: BeginInlineImage::operator_name(),
        parser: BeginInlineImage::read,
    },
    OperationMap {
        name: InlineImageData::operator_name(),
        parser: InlineImageData::read,
    },
    OperationMap {
        name: EndInlineImage::operator_name(),
        parser: EndInlineImage::read,
    },
];
