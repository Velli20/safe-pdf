use crate::{
    clipping_path_operators::*,
    color_operators::*,
    error::PdfOperatorError,
    graphics_state_operators::*,
    marked_content_operators::*,
    path_operators::*,
    path_paint_operators::*,
    pdf_operator::{Operands, PdfOperator, PdfOperatorVariant},
    shadings_operators::PaintShading,
    text_object_operators::*,
    text_positioning_operators::*,
    text_showing_operators::*,
    text_state_operators::*,
    type3_font_operators::SetCharWidthAndBoundingBox,
    xobject_and_image_operators::*,
};

/// Defines a mapping between a PDF operator's string representation (e.g., "m" for MoveTo)
/// and a function that can construct that operator an array of operands.
/// This is used to dynamically dispatch to the correct parsing logic based on the operator
/// encountered in the PDF content.
pub struct OpDescriptor {
    pub name: &'static str,
    pub operand_count: Option<usize>,
    pub parser: fn(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError>,
}

impl OpDescriptor {
    const fn from<T: PdfOperator>() -> Self {
        Self {
            name: T::NAME,
            operand_count: T::OPERAND_COUNT,
            parser: T::read,
        }
    }
}

pub(crate) const READ_MAP: &[OpDescriptor] = &[
    OpDescriptor::from::<ClipNonZero>(),
    OpDescriptor::from::<ClipEvenOdd>(),
    OpDescriptor::from::<SetGrayFill>(),
    OpDescriptor::from::<SetGrayStroke>(),
    OpDescriptor::from::<SetRGBFill>(),
    OpDescriptor::from::<SetRGBStroke>(),
    OpDescriptor::from::<SetCMYKFill>(),
    OpDescriptor::from::<SetCMYKStroke>(),
    OpDescriptor::from::<SetLineWidth>(),
    OpDescriptor::from::<SetLineCapStyle>(),
    OpDescriptor::from::<SetLineJoinStyle>(),
    OpDescriptor::from::<SetMiterLimit>(),
    OpDescriptor::from::<SetDashPattern>(),
    OpDescriptor::from::<SetGraphicsStateFromDict>(),
    OpDescriptor::from::<SaveGraphicsState>(),
    OpDescriptor::from::<RestoreGraphicsState>(),
    OpDescriptor::from::<ConcatMatrix>(),
    OpDescriptor::from::<BeginMarkedContent>(),
    OpDescriptor::from::<BeginMarkedContentWithProps>(),
    OpDescriptor::from::<EndMarkedContent>(),
    OpDescriptor::from::<MoveTo>(),
    OpDescriptor::from::<LineTo>(),
    OpDescriptor::from::<CurveTo>(),
    OpDescriptor::from::<CurveToV>(),
    OpDescriptor::from::<CurveToY>(),
    OpDescriptor::from::<ClosePath>(),
    OpDescriptor::from::<Rectangle>(),
    OpDescriptor::from::<StrokePath>(),
    OpDescriptor::from::<CloseStrokePath>(),
    OpDescriptor::from::<FillPathNonZero>(),
    OpDescriptor::from::<FillPathEvenOdd>(),
    OpDescriptor::from::<FillAndStrokePathNonZero>(),
    OpDescriptor::from::<FillAndStrokePathEvenOdd>(),
    OpDescriptor::from::<CloseFillAndStrokePathNonZero>(),
    OpDescriptor::from::<CloseFillAndStrokePathEvenOdd>(),
    OpDescriptor::from::<EndPath>(),
    OpDescriptor::from::<BeginText>(),
    OpDescriptor::from::<EndText>(),
    OpDescriptor::from::<MoveTextPosition>(),
    OpDescriptor::from::<MoveTextPositionAndSetLeading>(),
    OpDescriptor::from::<SetTextMatrix>(),
    OpDescriptor::from::<MoveToNextLine>(),
    OpDescriptor::from::<ShowText>(),
    OpDescriptor::from::<MoveNextLineShowText>(),
    OpDescriptor::from::<SetSpacingMoveShowText>(),
    OpDescriptor::from::<ShowTextArray>(),
    OpDescriptor::from::<SetCharacterSpacing>(),
    OpDescriptor::from::<SetWordSpacing>(),
    OpDescriptor::from::<SetHorizontalScaling>(),
    OpDescriptor::from::<SetLeading>(),
    OpDescriptor::from::<SetFont>(),
    OpDescriptor::from::<SetRenderingMode>(),
    OpDescriptor::from::<SetTextRise>(),
    OpDescriptor::from::<InvokeXObject>(),
    OpDescriptor::from::<BeginInlineImage>(),
    OpDescriptor::from::<InlineImageData>(),
    OpDescriptor::from::<EndInlineImage>(),
    OpDescriptor::from::<PaintShading>(),
    OpDescriptor::from::<SetCharWidthAndBoundingBox>(),
    OpDescriptor::from::<SetStrokeColorSpace>(),
    OpDescriptor::from::<SetNonStrokingColorSpace>(),
    OpDescriptor::from::<SetStrokingColor>(),
    OpDescriptor::from::<SetNonStrokingColor>(),
];

pub fn get_operation_descriptor(name: &str) -> Option<&'static OpDescriptor> {
    READ_MAP.iter().find(|op| op.name == name)
}
