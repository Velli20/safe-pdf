use pdf_object::{Value, comment::Comment};
use pdf_parser::{ParseObject, PdfParser};
use pdf_tokenizer::PdfToken;

use crate::{
    TextElement, clipping_path_operators::*, color_operators::*, error::PdfPainterError,
    graphics_state_operators::*, marked_content_operators::*, operation_map::READ_MAP,
    operator_tokenizer::OperatorReader, path_operators::*, path_paint_operators::*,
    text_object_operators::*, text_positioning_operators::*, text_showing_operators::*,
    text_state_operators::*, xobject_and_image_operators::*,
};

pub trait Op {
    const NAME: &'static str;
    const INSTRUCTION: &'static str;
    const OPERAND_COUNT: usize;
}

pub struct Operands<'a>(&'a [Value]);

impl Operands<'_> {
    pub fn get_f32(&mut self) -> Result<f32, PdfPainterError> {
        let value = self.0.get(0);
        self.0 = &self.0[1..];

        if let Some(Value::Number(number)) = value {
            if let Some(number) = number.as_f32() {
                Ok(number)
            } else {
                Err(PdfPainterError::InvalidOperandType)
            }
        } else {
            Err(PdfPainterError::InvalidOperandType)
        }
    }

    pub fn get_str(&mut self) -> Result<String, PdfPainterError> {
        let value = self.0.get(0);
        self.0 = &self.0[1..];

        if let Some(Value::LiteralString(string)) = value {
            Ok(string.0.clone())
        } else if let Some(Value::HexString(string)) = value {
            Ok(string.0.clone())
        } else {
            Err(PdfPainterError::InvalidOperandType)
        }
    }

    pub fn get_name(&mut self) -> Result<String, PdfPainterError> {
        let value = self.0.get(0);
        self.0 = &self.0[1..];

        if let Some(Value::Name(name)) = value {
            Ok(name.0.clone())
        } else {
            Err(PdfPainterError::InvalidOperandType)
        }
    }

    pub fn get_u8(&mut self) -> Result<u8, PdfPainterError> {
        let value = self.0.get(0);
        self.0 = &self.0[1..];

        if let Some(Value::Number(number)) = value {
            if let Some(number) = number.as_i64() {
                u8::try_from(number).map_err(|_| PdfPainterError::InvalidOperandType)
            } else {
                Err(PdfPainterError::InvalidOperandType)
            }
        } else {
            Err(PdfPainterError::InvalidOperandType)
        }
    }

    pub fn get_text_element_array(&mut self) -> Result<Vec<TextElement>, PdfPainterError> {
        let value = self.0.get(0);
        self.0 = &self.0[1..];

        if let Some(Value::Array(array_values)) = value {
            let mut elements = Vec::new();
            for val_obj in &array_values.0 {
                match val_obj {
                    Value::LiteralString(s) => {
                        elements.push(TextElement::Text { value: s.0.clone() });
                    }
                    Value::Number(n) => {
                        if let Some(num_f32) = n.as_f32() {
                            elements.push(TextElement::Adjustment { amount: num_f32 });
                        } else {
                            return Err(PdfPainterError::InvalidOperandType);
                        }
                    }
                    _ => return Err(PdfPainterError::InvalidOperandType),
                }
            }
            Ok(elements)
        } else {
            Err(PdfPainterError::InvalidOperandType)
        }
    }

    pub fn get_f32_array(&mut self) -> Result<Vec<f32>, PdfPainterError> {
        let value = self.0.get(0);
        self.0 = &self.0[1..];

        if let Some(Value::Array(array_values)) = value {
            let mut numbers = Vec::new();
            for val_obj in &array_values.0 {
                match val_obj {
                    Value::Number(n) => {
                        if let Some(num_f32) = n.as_f32() {
                            numbers.push(num_f32);
                        } else {
                            return Err(PdfPainterError::InvalidOperandType);
                        }
                    }
                    _ => return Err(PdfPainterError::InvalidOperandType),
                }
            }
            Ok(numbers)
        } else {
            Err(PdfPainterError::InvalidOperandType)
        }
    }
}

/// Represents all possible PDF content stream operators defined within this crate.
///
/// This enum acts as a sum type, allowing for type-safe handling of various
/// PDF operators. Each variant holds the specific operator struct, which in turn
/// contains its operands and associated methods.
///
/// Each operator performs a specific function, such as
/// drawing a line, displaying text, or setting a color.
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
}

impl PdfOperatorVariant {
    pub fn from(input: &[u8]) -> Result<Vec<PdfOperatorVariant>, PdfPainterError> {
        let mut parser = PdfParser::from(input);
        let mut operators = Vec::new();
        let mut operands = Vec::new();

        loop {
            parser.skip_whitespace();

            if let Some(PdfToken::Percent) = parser.tokenizer.peek()? {
                let _comment: Comment = parser.parse()?;
                continue;
            }

            let peeked = parser.tokenizer.peek()?;
            if peeked.is_none() {
                break;
            }

            if let Some(PdfToken::Alphabetic(_)) = &peeked {
                let name = parser.read_operation_name()?;
                if name.is_empty() {
                    break;
                }

                for operation in READ_MAP {
                    if name == operation.name {
                        let mut ops = Operands(operands.as_slice());
                        let operator = (operation.parser)(&mut ops)?;
                        operators.push(operator);

                        // Clear operands after they've been consumed.
                        operands.clear();
                        break;
                    }
                }
                continue;
            }

            let value = parser.parse_object()?;
            operands.push(value);
        }

        Ok(operators)
    }
}

#[cfg(test)]
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
                    PdfOperatorVariant::ClosePath(ClosePath::new()),
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
                    PdfOperatorVariant::ClosePath(ClosePath::new()),
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
                    PdfOperatorVariant::StrokePath(StrokePath::new()),
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
                    PdfOperatorVariant::ClosePath(ClosePath::new()),
                    PdfOperatorVariant::FillPathNonZero(FillPathNonZero::new()),
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
                    PdfOperatorVariant::ClosePath(ClosePath::new()),
                    PdfOperatorVariant::FillPathEvenOdd(FillPathEvenOdd::new()),
                ],
            },
            TestCase {
                description: "17. Path construction followed by Stroke and Close (s)",
                input: b"10 10 m 50 10 l 50 50 l s",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(50.0, 50.0)),
                    PdfOperatorVariant::CloseStrokePath(CloseStrokePath::new()),
                ],
            },
            TestCase {
                description: "18. Path construction followed by Fill and Stroke (B)",
                input: b"10 10 100 50 re B",
                expected_ops: vec![
                    PdfOperatorVariant::Rectangle(Rectangle::new(10.0, 10.0, 100.0, 50.0)),
                    PdfOperatorVariant::FillAndStrokePathNonZero(FillAndStrokePathNonZero::new()),
                ],
            },
            TestCase {
                description: "19. Path construction followed by Fill and Stroke EvenOdd (B*)",
                input: b"10 10 100 50 re B*",
                expected_ops: vec![
                    PdfOperatorVariant::Rectangle(Rectangle::new(10.0, 10.0, 100.0, 50.0)),
                    PdfOperatorVariant::FillAndStrokePathEvenOdd(FillAndStrokePathEvenOdd::new()),
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
                        CloseFillAndStrokePathNonZero::new(),
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
                        CloseFillAndStrokePathEvenOdd::new(),
                    ),
                ],
            },
            TestCase {
                description: "22. Path construction followed by End Path (n)",
                input: b"10 10 m 100 100 l n",
                expected_ops: vec![
                    PdfOperatorVariant::MoveTo(MoveTo::new(10.0, 10.0)),
                    PdfOperatorVariant::LineTo(LineTo::new(100.0, 100.0)),
                    PdfOperatorVariant::EndPath(EndPath::new()),
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
                    PdfOperatorVariant::ClosePath(ClosePath::new()),
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
                    PdfOperatorVariant::StrokePath(StrokePath::new()),
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
