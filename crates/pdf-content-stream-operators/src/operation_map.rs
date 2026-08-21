use crate::{
    clipping_path_operators::*,
    color_operators::*,
    compatibility_operators::*,
    error::PdfOperatorError,
    graphics_state_operators::*,
    marked_content_operators::*,
    operands::Operands,
    operator_trait::PdfOperator,
    path_operators::*,
    path_paint_operators::*,
    shadings_operators::PaintShading,
    text_object_operators::*,
    text_positioning_operators::*,
    text_showing_operators::*,
    text_state_operators::*,
    type3_font_operators::{SetCharWidth, SetCharWidthAndBoundingBox},
    variants::PdfOperatorVariant,
    xobject_and_image_operators::*,
};

use pdf_image::InlineImage;
use pdf_parser::parser::PdfParser;

/// Custom parser used by operators whose bytes are not represented by the
/// regular operand list.
pub type OperatorParseHook =
    for<'a> fn(&mut PdfParser<'a>) -> Result<Option<PdfOperatorVariant>, PdfOperatorError>;

/// Defines a mapping between a PDF operator's string representation (e.g., "m" for MoveTo)
/// and a function that can construct that operator an array of operands.
/// This is used to dynamically dispatch to the correct parsing logic based on the operator
/// encountered in the PDF content.
#[derive(Clone, Copy)]
pub struct OpDescriptor {
    pub operand_count: Option<usize>,
    pub parser: fn(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError>,
    pub parse_hook: Option<OperatorParseHook>,
}

impl OpDescriptor {
    const fn from<T: PdfOperator>() -> Self {
        Self {
            operand_count: T::OPERAND_COUNT,
            parser: T::read,
            parse_hook: None,
        }
    }

    const fn with_parse_hook<T: PdfOperator>() -> Self {
        Self {
            operand_count: T::OPERAND_COUNT,
            parser: T::read,
            parse_hook: Some(T::parse),
        }
    }
}

/// Returns the parser descriptor for a PDF content-stream operator.
pub fn get_operation_descriptor(name: &[u8]) -> Option<OpDescriptor> {
    match name {
        b"\"" => Some(OpDescriptor::from::<SetSpacingMoveShowText>()),
        b"'" => Some(OpDescriptor::from::<MoveNextLineShowText>()),
        b"B" => Some(OpDescriptor::from::<FillAndStrokePathNonZero>()),
        b"B*" => Some(OpDescriptor::from::<FillAndStrokePathEvenOdd>()),
        b"BDC" => Some(OpDescriptor::from::<BeginMarkedContentWithProps>()),
        b"BI" => Some(OpDescriptor::with_parse_hook::<InlineImage>()),
        b"BMC" => Some(OpDescriptor::from::<BeginMarkedContent>()),
        b"BT" => Some(OpDescriptor::from::<BeginText>()),
        b"BX" => Some(OpDescriptor::from::<BeginCompatibility>()),
        b"CS" => Some(OpDescriptor::from::<SetStrokeColorSpace>()),
        b"Do" => Some(OpDescriptor::from::<InvokeXObject>()),
        b"EMC" => Some(OpDescriptor::from::<EndMarkedContent>()),
        b"ET" => Some(OpDescriptor::from::<EndText>()),
        b"EX" => Some(OpDescriptor::from::<EndCompatibility>()),
        b"G" => Some(OpDescriptor::from::<SetGrayStroke>()),
        b"J" => Some(OpDescriptor::from::<SetLineCapStyle>()),
        b"K" => Some(OpDescriptor::from::<SetCMYKStroke>()),
        b"M" => Some(OpDescriptor::from::<SetMiterLimit>()),
        b"Q" => Some(OpDescriptor::from::<RestoreGraphicsState>()),
        b"RG" => Some(OpDescriptor::from::<SetRGBStroke>()),
        b"S" => Some(OpDescriptor::from::<StrokePath>()),
        b"SC" => Some(OpDescriptor::from::<SetStrokingColorSc>()),
        b"SCN" => Some(OpDescriptor::from::<SetStrokingColor>()),
        b"T*" => Some(OpDescriptor::from::<MoveToNextLine>()),
        b"TD" => Some(OpDescriptor::from::<MoveTextPositionAndSetLeading>()),
        b"TJ" => Some(OpDescriptor::from::<ShowTextArray>()),
        b"TL" => Some(OpDescriptor::from::<SetLeading>()),
        b"Tc" => Some(OpDescriptor::from::<SetCharacterSpacing>()),
        b"Td" => Some(OpDescriptor::from::<MoveTextPosition>()),
        b"Tf" => Some(OpDescriptor::from::<SetFont>()),
        b"Tj" => Some(OpDescriptor::from::<ShowText>()),
        b"Tm" => Some(OpDescriptor::from::<SetTextMatrix>()),
        b"Tr" => Some(OpDescriptor::from::<SetRenderingMode>()),
        b"Ts" => Some(OpDescriptor::from::<SetTextRise>()),
        b"Tw" => Some(OpDescriptor::from::<SetWordSpacing>()),
        b"Tz" => Some(OpDescriptor::from::<SetHorizontalScaling>()),
        b"W" => Some(OpDescriptor::from::<ClipNonZero>()),
        b"W*" => Some(OpDescriptor::from::<ClipEvenOdd>()),
        b"b" => Some(OpDescriptor::from::<CloseFillAndStrokePathNonZero>()),
        b"b*" => Some(OpDescriptor::from::<CloseFillAndStrokePathEvenOdd>()),
        b"c" => Some(OpDescriptor::from::<CurveTo>()),
        b"cm" => Some(OpDescriptor::from::<ConcatMatrix>()),
        b"cs" => Some(OpDescriptor::from::<SetNonStrokingColorSpace>()),
        b"d" => Some(OpDescriptor::from::<SetDashPattern>()),
        b"d0" => Some(OpDescriptor::from::<SetCharWidth>()),
        b"d1" => Some(OpDescriptor::from::<SetCharWidthAndBoundingBox>()),
        b"f" => Some(OpDescriptor::from::<FillPathNonZero>()),
        b"f*" => Some(OpDescriptor::from::<FillPathEvenOdd>()),
        b"g" => Some(OpDescriptor::from::<SetGrayFill>()),
        b"gs" => Some(OpDescriptor::from::<SetGraphicsStateFromDict>()),
        b"h" => Some(OpDescriptor::from::<ClosePath>()),
        b"i" => Some(OpDescriptor::from::<SetFlatnessTolerance>()),
        b"j" => Some(OpDescriptor::from::<SetLineJoinStyle>()),
        b"k" => Some(OpDescriptor::from::<SetCMYKFill>()),
        b"l" => Some(OpDescriptor::from::<LineTo>()),
        b"m" => Some(OpDescriptor::from::<MoveTo>()),
        b"n" => Some(OpDescriptor::from::<EndPath>()),
        b"q" => Some(OpDescriptor::from::<SaveGraphicsState>()),
        b"re" => Some(OpDescriptor::from::<Rectangle>()),
        b"rg" => Some(OpDescriptor::from::<SetRGBFill>()),
        b"ri" => Some(OpDescriptor::from::<SetRenderingIntent>()),
        b"s" => Some(OpDescriptor::from::<CloseStrokePath>()),
        b"sc" => Some(OpDescriptor::from::<SetNonStrokingColorSc>()),
        b"scn" => Some(OpDescriptor::from::<SetNonStrokingColor>()),
        b"sh" => Some(OpDescriptor::from::<PaintShading>()),
        b"v" => Some(OpDescriptor::from::<CurveToV>()),
        b"w" => Some(OpDescriptor::from::<SetLineWidth>()),
        b"y" => Some(OpDescriptor::from::<CurveToY>()),
        _ => None,
    }
}
