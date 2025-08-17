use pdf_parser::{PdfParser, traits::CommentParser};
use pdf_tokenizer::PdfToken;

use crate::{
    clipping_path_operators::*, color_operators::*, error::PdfOperatorError,
    graphics_state_operators::*, marked_content_operators::*, operation_map::READ_MAP,
    operator_tokenizer::OperatorReader, path_operators::*, path_paint_operators::*,
    pdf_operator_backend::PdfOperatorBackend, shadings_operators::PaintShading,
    text_object_operators::*, text_positioning_operators::*, text_showing_operators::*,
    text_state_operators::*, type3_font_operators::SetCharWidthAndBoundingBox,
    xobject_and_image_operators::*,
};

use super::{Operands, PdfOperator};

#[derive(Debug, Clone, PartialEq)]
pub enum PdfOperatorVariant {
    LineTo(LineTo),
    MoveTo(MoveTo),
    CurveTo(CurveTo),
    CurveToV(CurveToV),
    CurveToY(CurveToY),
    ClosePath(ClosePath),
    Rectangle(Rectangle),
    StrokePath(StrokePath),
    CloseStrokePath(CloseStrokePath),
    FillPathNonZero(FillPathNonZero),
    FillPathEvenOdd(FillPathEvenOdd),
    FillAndStrokePathNonZero(FillAndStrokePathNonZero),
    FillAndStrokePathEvenOdd(FillAndStrokePathEvenOdd),
    CloseFillAndStrokePathNonZero(CloseFillAndStrokePathNonZero),
    CloseFillAndStrokePathEvenOdd(CloseFillAndStrokePathEvenOdd),
    EndPath(EndPath),
    ClipNonZero(ClipNonZero),
    ClipEvenOdd(ClipEvenOdd),
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
    SetGraphicsStateFromDict(SetGraphicsStateFromDict),
    SaveGraphicsState(SaveGraphicsState),
    RestoreGraphicsState(RestoreGraphicsState),
    ConcatMatrix(ConcatMatrix),
    BeginMarkedContent(BeginMarkedContent),
    BeginMarkedContentWithProps(BeginMarkedContentWithProps),
    EndMarkedContent(EndMarkedContent),
    BeginText(BeginText),
    EndText(EndText),
    MoveTextPosition(MoveTextPosition),
    MoveTextPositionAndSetLeading(MoveTextPositionAndSetLeading),
    SetTextMatrix(SetTextMatrix),
    MoveToNextLine(MoveToNextLine),
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
    BeginInlineImage(BeginInlineImage),
    InlineImageData(InlineImageData),
    EndInlineImage(EndInlineImage),
    PaintShading(PaintShading),
    SetCharWidthAndBoundingBox(SetCharWidthAndBoundingBox),
    SetStrokeColorSpace(SetStrokeColorSpace),
    SetNonStrokingColorSpace(SetNonStrokingColorSpace),
    SetStrokingColor(SetStrokingColor),
    SetNonStrokingColor(SetNonStrokingColor),
}

impl PdfOperatorVariant {
    pub fn from(input: &[u8]) -> Result<Vec<PdfOperatorVariant>, PdfOperatorError> {
        let mut parser = PdfParser::from(input);
        let mut operators = Vec::new();
        let mut operands = Vec::new();

        loop {
            parser.skip_whitespace();

            if let Some(PdfToken::Percent) = parser.tokenizer.peek() {
                parser.parse_comment()?;
                continue;
            }

            let peeked = parser.tokenizer.peek();
            if peeked.is_none() {
                break;
            }

            if let Some(PdfToken::Alphabetic(_)) = &peeked {
                let name = parser.read_operation_name()?;
                if name.is_empty() {
                    break;
                }

                let mut handled = false;
                for operation in READ_MAP {
                    if name == operation.name {
                        match operation.operand_count {
                            Some(required_count) if operands.len() != required_count => {
                                return Err(PdfOperatorError::IncorrectOperandCount {
                                    op_name: operation.name,
                                    got: operands.len(),
                                    expected: required_count,
                                });
                            }
                            _ => {}
                        }

                        let mut ops = Operands {
                            values: operands.as_slice(),
                        };
                        let operator = (operation.parser)(&mut ops)?;
                        operators.push(operator);
                        handled = true;
                        operands.clear();
                        break;
                    }
                }
                if !handled {
                    return Err(PdfOperatorError::UnknownOperator(name.to_string()));
                }
                continue;
            }

            let value = parser.parse_object()?;
            operands.push(value);
        }

        Ok(operators)
    }

    pub fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), T::ErrorType> {
        match self {
            PdfOperatorVariant::LineTo(op) => op.call(backend),
            PdfOperatorVariant::MoveTo(op) => op.call(backend),
            PdfOperatorVariant::CurveTo(op) => op.call(backend),
            PdfOperatorVariant::CurveToV(op) => op.call(backend),
            PdfOperatorVariant::CurveToY(op) => op.call(backend),
            PdfOperatorVariant::ClosePath(op) => op.call(backend),
            PdfOperatorVariant::Rectangle(op) => op.call(backend),
            PdfOperatorVariant::StrokePath(op) => op.call(backend),
            PdfOperatorVariant::CloseStrokePath(op) => op.call(backend),
            PdfOperatorVariant::FillPathNonZero(op) => op.call(backend),
            PdfOperatorVariant::FillPathEvenOdd(op) => op.call(backend),
            PdfOperatorVariant::FillAndStrokePathNonZero(op) => op.call(backend),
            PdfOperatorVariant::FillAndStrokePathEvenOdd(op) => op.call(backend),
            PdfOperatorVariant::CloseFillAndStrokePathNonZero(op) => op.call(backend),
            PdfOperatorVariant::CloseFillAndStrokePathEvenOdd(op) => op.call(backend),
            PdfOperatorVariant::EndPath(op) => op.call(backend),
            PdfOperatorVariant::ClipNonZero(op) => op.call(backend),
            PdfOperatorVariant::ClipEvenOdd(op) => op.call(backend),
            PdfOperatorVariant::SetGrayFill(op) => op.call(backend),
            PdfOperatorVariant::SetGrayStroke(op) => op.call(backend),
            PdfOperatorVariant::SetRGBFill(op) => op.call(backend),
            PdfOperatorVariant::SetRGBStroke(op) => op.call(backend),
            PdfOperatorVariant::SetCMYKFill(op) => op.call(backend),
            PdfOperatorVariant::SetCMYKStroke(op) => op.call(backend),
            PdfOperatorVariant::SetLineWidth(op) => op.call(backend),
            PdfOperatorVariant::SetLineCapStyle(op) => op.call(backend),
            PdfOperatorVariant::SetLineJoinStyle(op) => op.call(backend),
            PdfOperatorVariant::SetMiterLimit(op) => op.call(backend),
            PdfOperatorVariant::SetDashPattern(op) => op.call(backend),
            PdfOperatorVariant::SetGraphicsStateFromDict(op) => op.call(backend),
            PdfOperatorVariant::SaveGraphicsState(op) => op.call(backend),
            PdfOperatorVariant::RestoreGraphicsState(op) => op.call(backend),
            PdfOperatorVariant::ConcatMatrix(op) => op.call(backend),
            PdfOperatorVariant::BeginMarkedContent(op) => op.call(backend),
            PdfOperatorVariant::BeginMarkedContentWithProps(op) => op.call(backend),
            PdfOperatorVariant::EndMarkedContent(op) => op.call(backend),
            PdfOperatorVariant::BeginText(op) => op.call(backend),
            PdfOperatorVariant::EndText(op) => op.call(backend),
            PdfOperatorVariant::MoveTextPosition(op) => op.call(backend),
            PdfOperatorVariant::MoveTextPositionAndSetLeading(op) => op.call(backend),
            PdfOperatorVariant::SetTextMatrix(op) => op.call(backend),
            PdfOperatorVariant::MoveToNextLine(op) => op.call(backend),
            PdfOperatorVariant::ShowText(op) => op.call(backend),
            PdfOperatorVariant::MoveNextLineShowText(op) => op.call(backend),
            PdfOperatorVariant::SetSpacingMoveShowText(op) => op.call(backend),
            PdfOperatorVariant::ShowTextArray(op) => op.call(backend),
            PdfOperatorVariant::SetCharacterSpacing(op) => op.call(backend),
            PdfOperatorVariant::SetWordSpacing(op) => op.call(backend),
            PdfOperatorVariant::SetHorizontalScaling(op) => op.call(backend),
            PdfOperatorVariant::SetLeading(op) => op.call(backend),
            PdfOperatorVariant::SetFont(op) => op.call(backend),
            PdfOperatorVariant::SetRenderingMode(op) => op.call(backend),
            PdfOperatorVariant::SetTextRise(op) => op.call(backend),
            PdfOperatorVariant::InvokeXObject(op) => op.call(backend),
            PdfOperatorVariant::BeginInlineImage(op) => op.call(backend),
            PdfOperatorVariant::InlineImageData(op) => op.call(backend),
            PdfOperatorVariant::EndInlineImage(op) => op.call(backend),
            PdfOperatorVariant::PaintShading(op) => op.call(backend),
            PdfOperatorVariant::SetCharWidthAndBoundingBox(op) => op.call(backend),
            PdfOperatorVariant::SetStrokeColorSpace(op) => op.call(backend),
            PdfOperatorVariant::SetNonStrokingColorSpace(op) => op.call(backend),
            PdfOperatorVariant::SetStrokingColor(op) => op.call(backend),
            PdfOperatorVariant::SetNonStrokingColor(op) => op.call(backend),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        struct TestCase<'a> {
            description: &'a str,
            input: &'a [u8],
            expected_ops: Vec<PdfOperatorVariant>,
        }

        let test_cases = vec![
            TestCase {
                description: "0. ConcatMatrix(cm)",
                input: b"\n.17576218 0 0 .17576218 2227.4995 159.375 cm",
                expected_ops: vec![PdfOperatorVariant::ConcatMatrix(ConcatMatrix::new([
                    0.17576218, 0.0, 0.0, 0.17576218, 2227.4995, 159.375,
                ]))],
            },
            TestCase {
                description: "1. Simple moveto (m)",
                input: b"100 100 m",
                expected_ops: vec![PdfOperatorVariant::MoveTo(MoveTo::new(100.0, 100.0))],
            },
            TestCase {
                description: "2. Moveto with real numbers",
                input: b"50.5 75.2 m",
                expected_ops: vec![PdfOperatorVariant::MoveTo(MoveTo::new(50.5, 75.2))],
            },
            TestCase {
                description: "3. Moveto with negative coordinates",
                input: b"-10 -20 m",
                expected_ops: vec![PdfOperatorVariant::MoveTo(MoveTo::new(-10.0, -20.0))],
            },
            TestCase {
                description: "4. Moveto followed by lineto (l)",
                input: b"10 10 m 200 50 l",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(200.0, 50.0)),
                ],
            },
            TestCase {
                description: "5. Multiple lineto operations",
                input: b"10 10 m 50 10 l 50 50 l 10 50 l",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 50.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(10.0, 50.0)),
                ],
            },
            TestCase {
                description: "6. Simple closepath (h) after drawing lines",
                input: b"10 10 m 50 10 l 50 50 l h",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 50.0)),
                    PdfOperatorVariant::ClosePath(ClosePath),
                ],
            },
            TestCase {
                description: "7. Simple rectangle (re)",
                input: b"50 50 100 75 re",
                expected_ops: vec![PdfOperatorVariant::Rectangle(Rectangle::new(
                    50.0, 50.0, 100.0, 75.0,
                ))],
            },
            TestCase {
                description: "8. Simple Bézier curve (c)",
                input: b"0 0 m 10 10 90 10 100 0 c",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(0.0, 0.0)),
                    PdfOperatorVariant::CurveTo(CurveTo::new(10.0, 10.0, 90.0, 10.0, 100.0, 0.0)),
                ],
            },
            TestCase {
                description: "9. Input with comments",
                input:
                    b"% initial comment\n10 20 m % moveto\n % another comment\n 30 40 l % lineto\n",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 20.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(30.0, 40.0)),
                ],
            },
            TestCase {
                description: "10. Empty input",
                input: b"",
                expected_ops: vec![],
            },
            TestCase {
                description: "11. Comments and whitespace only",
                input: b" % first comment \n % second comment \n ",
                expected_ops: vec![],
            },
            TestCase {
                description: "11. Multiple operators with varied spacing",
                input: b" 10 10 m \n 20 20 l \r\n 30 30 l h ",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(20.0, 20.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(30.0, 30.0)),
                    PdfOperatorVariant::ClosePath(ClosePath),
                ],
            },
            TestCase {
                description: "12. Multiple subpaths (multiple 'm' operators)",
                input: b"10 10 m 50 50 l 100 100 m 150 150 l",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 50.0)),
                    PdfOperatorVariant::MoveTo(MoveTo::new(100.0, 100.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(150.0, 150.0)),
                ],
            },
            TestCase {
                description: "13. Rectangle followed by moveto/lineto",
                input: b"10 10 50 50 re 70 70 m 100 100 l",
                expected_ops: vec![
                    PdfOperatorVariant::Rectangle(Rectangle::new(10.0, 10.0, 50.0, 50.0)),
                    PdfOperatorVariant::MoveTo(MoveTo::new(70.0, 70.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(100.0, 100.0)),
                ],
            },
            TestCase {
                description: "14. Path construction followed by Stroke (S)",
                input: b"10 10 m 100 100 l S",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(100.0, 100.0)),
                    PdfOperatorVariant::StrokePath(StrokePath),
                ],
            },
            TestCase {
                description: "15. Path construction followed by Fill (f)",
                input: b"10 10 m 50 10 l 50 50 l 10 50 l h f",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 50.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(10.0, 50.0)),
                    PdfOperatorVariant::ClosePath(ClosePath),
                    PdfOperatorVariant::FillPathNonZero(FillPathNonZero),
                ],
            },
            TestCase {
                description: "16. Path construction followed by Fill EvenOdd (f*)",
                input: b"10 10 m 50 10 l 50 50 l 10 50 l h f*",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 50.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(10.0, 50.0)),
                    PdfOperatorVariant::ClosePath(ClosePath),
                    PdfOperatorVariant::FillPathEvenOdd(FillPathEvenOdd),
                ],
            },
            TestCase {
                description: "17. Path construction followed by Stroke and Close (s)",
                input: b"10 10 m 50 10 l 50 50 l s",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 50.0)),
                    PdfOperatorVariant::CloseStrokePath(CloseStrokePath),
                ],
            },
            TestCase {
                description: "18. Path construction followed by Fill and Stroke (B)",
                input: b"10 10 100 50 re B",
                expected_ops: vec![
                    PdfOperatorVariant::Rectangle(Rectangle::new(10.0, 10.0, 100.0, 50.0)),
                    PdfOperatorVariant::FillAndStrokePathNonZero(FillAndStrokePathNonZero),
                ],
            },
            TestCase {
                description: "19. Path construction followed by Fill and Stroke EvenOdd (B*)",
                input: b"10 10 100 50 re B*",
                expected_ops: vec![
                    PdfOperatorVariant::Rectangle(Rectangle::new(10.0, 10.0, 100.0, 50.0)),
                    PdfOperatorVariant::FillAndStrokePathEvenOdd(FillAndStrokePathEvenOdd),
                ],
            },
            TestCase {
                description: "20. Path construction followed by Close, Fill and Stroke (b)",
                input: b"10 10 m 50 10 l 50 50 l b",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 50.0)),
                    PdfOperatorVariant::CloseFillAndStrokePathNonZero(
                        CloseFillAndStrokePathNonZero,
                    ),
                ],
            },
            TestCase {
                description: "21. Path construction followed by Close, Fill and Stroke EvenOdd (b*)",
                input: b"10 10 m 50 10 l 50 50 l b*",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 50.0)),
                    PdfOperatorVariant::CloseFillAndStrokePathEvenOdd(
                        CloseFillAndStrokePathEvenOdd,
                    ),
                ],
            },
            TestCase {
                description: "22. Path construction followed by End Path (n)",
                input: b"10 10 m 100 100 l n",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(100.0, 100.0)),
                    PdfOperatorVariant::EndPath(EndPath),
                ],
            },
            TestCase {
                description: "23. Complex sequence with curves and lines",
                input: b"0 0 m 50 100 l 100 0 150 100 200 0 c 250 -50 300 0 y h",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(0.0, 0.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 100.0)),
                    PdfOperatorVariant::CurveTo(CurveTo::new(100.0, 0.0, 150.0, 100.0, 200.0, 0.0)),
                    PdfOperatorVariant::CurveToY(CurveToY::new(250.0, -50.0, 300.0, 0.0)),
                    PdfOperatorVariant::ClosePath(ClosePath),
                ],
            },
            TestCase {
                description: "24. Multiple paths with stroke",
                input: b"10 10 m 100 100 l 200 200 m 300 300 l S",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(100.0, 100.0)),
                    PdfOperatorVariant::MoveTo(MoveTo::new(200.0, 200.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(300.0, 300.0)),
                    PdfOperatorVariant::StrokePath(StrokePath),
                ],
            },
        ];

        for tc in test_cases {
            let actual_ops = PdfOperatorVariant::from(tc.input).unwrap_or_else(|e| {
                panic!(
                    "Failed for test '{}': {:?}, input: '{}'",
                    tc.description,
                    e,
                    String::from_utf8_lossy(tc.input)
                );
            });
            assert_eq!(
                actual_ops,
                tc.expected_ops,
                "Mismatch for test: '{}', input: '{}'",
                tc.description,
                String::from_utf8_lossy(tc.input)
            );
        }
    }
}
