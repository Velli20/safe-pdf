use pdf_image::InlineImage;

use crate::compatibility_operators::{BeginCompatibility, EndCompatibility};
use crate::type3_font_operators::SetCharWidth;
use crate::{
    clipping_path_operators::*,
    color_operators::*,
    error::PdfOperatorError,
    graphics_state_operators::*,
    marked_content_operators::*,
    path_operators::*,
    path_paint_operators::*,
    pdf_operator_backend::{BackendError, PdfOperatorBackend},
    shadings_operators::PaintShading,
    text_object_operators::*,
    text_positioning_operators::*,
    text_showing_operators::*,
    text_state_operators::*,
    type3_font_operators::SetCharWidthAndBoundingBox,
    xobject_and_image_operators::*,
};

use super::PdfOperator;
use super::operator_stream_parser::OperatorStreamParser;

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
    SetFlatnessTolerance(SetFlatnessTolerance),
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
    BeginCompatibility(BeginCompatibility),
    InlineImage(InlineImage),
    EndCompatibility(EndCompatibility),
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
    /// Parses a PDF content stream and returns a vector of operators.
    ///
    /// # Errors
    /// Returns an error if an unknown operator is encountered, an operator has an
    /// incorrect number of operands, or the content stream contains malformed data.
    pub fn parse(input: &[u8]) -> Result<Vec<PdfOperatorVariant>, PdfOperatorError> {
        let mut operators = Vec::new();
        Self::parse_into(input, &mut operators)?;
        Ok(operators)
    }

    /// Parses a PDF content stream and appends the resulting operators into `out`.
    ///
    /// Sharing a single output buffer across multiple streams (e.g. when a page's
    /// `/Contents` is an array of streams) avoids per-stream allocations.
    pub(crate) fn parse_into(
        input: &[u8],
        out: &mut Vec<PdfOperatorVariant>,
    ) -> Result<(), PdfOperatorError> {
        let mut parser = OperatorStreamParser::new(input, out);
        while parser.parse_next_item()? {}

        Ok(())
    }

    pub fn call<T: PdfOperatorBackend>(&self, backend: &mut T) -> Result<(), BackendError<T>> {
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
            PdfOperatorVariant::SetFlatnessTolerance(op) => op.call(backend),
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
            PdfOperatorVariant::BeginCompatibility(op) => op.call(backend),
            PdfOperatorVariant::InlineImage(op) => op.call(backend),
            PdfOperatorVariant::EndCompatibility(op) => op.call(backend),
            PdfOperatorVariant::PaintShading(op) => op.call(backend),
            PdfOperatorVariant::SetCharWidthAndBoundingBox(op) => op.call(backend),
            PdfOperatorVariant::SetCharWidth(op) => op.call(backend),
            PdfOperatorVariant::SetRenderingIntent(op) => op.call(backend),
            PdfOperatorVariant::SetStrokeColorSpace(op) => op.call(backend),
            PdfOperatorVariant::SetNonStrokingColorSpace(op) => op.call(backend),
            PdfOperatorVariant::SetStrokingColor(op) => op.call(backend),
            PdfOperatorVariant::SetNonStrokingColor(op) => op.call(backend),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::recording_pdf_operator_backend::{RecordedOperation, RecordingBackend};
    use pdf_object::object_variant::ObjectVariant;

    use super::*;

    #[test]
    fn test_bug_727() {
        let input = b"[ (2.) 1 (0) 1 (!)\n2 (3) 1 (4) 1 (4) 1 (0) 1 (0) 1 (#) 2 (%) 2 (%) 2 (.) 1 (\\)) 2 (4) ]  TJ";
        let result = PdfOperatorVariant::parse(input);
        assert!(result.is_ok());
    }

    #[test]
    fn parses_inline_image_and_resumes_after_ei() {
        let input = b"q BI /IM true /W 1 /H 1 /BPC 8 ID \x00\x01\x02\nEI Q Q";

        let result = PdfOperatorVariant::parse(input).unwrap();

        assert_eq!(
            result,
            vec![
                PdfOperatorVariant::SaveGraphicsState(SaveGraphicsState),
                PdfOperatorVariant::InlineImage(InlineImage::new(
                    pdf_object::dictionary::Dictionary::new(std::collections::BTreeMap::from([
                        ("BPC".to_string(), ObjectVariant::Integer(8)),
                        ("H".to_string(), ObjectVariant::Integer(1)),
                        ("IM".to_string(), ObjectVariant::Boolean(true)),
                        ("W".to_string(), ObjectVariant::Integer(1)),
                    ])),
                    vec![0x00, 0x01, 0x02, b'\n'],
                )),
                PdfOperatorVariant::RestoreGraphicsState(RestoreGraphicsState),
                PdfOperatorVariant::RestoreGraphicsState(RestoreGraphicsState),
            ]
        );
    }

    #[test]
    fn inline_image_dictionary_boolean_values_do_not_become_operators() {
        let input = b"BI /IM true /Mask false /Intent null ID x\nEI Q";

        let result = PdfOperatorVariant::parse(input).unwrap();

        assert_eq!(result.len(), 2);
        match result.first() {
            Some(PdfOperatorVariant::InlineImage(image)) => {
                assert_eq!(
                    &image.dictionary().dictionary,
                    &std::collections::BTreeMap::from([
                        ("IM".to_string(), ObjectVariant::Boolean(true)),
                        ("Intent".to_string(), ObjectVariant::Null),
                        ("Mask".to_string(), ObjectVariant::Boolean(false)),
                    ])
                );
                assert_eq!(image.data(), b"x\n");
            }
            other => panic!("expected inline image, got {other:?}"),
        }
        assert!(matches!(
            result.get(1),
            Some(PdfOperatorVariant::RestoreGraphicsState(
                RestoreGraphicsState
            ))
        ));
    }

    #[test]
    fn inline_image_data_does_not_stop_at_embedded_ei_bytes() {
        let input = b"BI /W 1 /H 1 ID abcEIxdef\nEI Q";

        let result = PdfOperatorVariant::parse(input).unwrap();

        match result.first() {
            Some(PdfOperatorVariant::InlineImage(image)) => {
                assert_eq!(image.data(), b"abcEIxdef\n");
                assert_eq!(
                    &image.dictionary().dictionary,
                    &std::collections::BTreeMap::from([
                        ("H".to_string(), ObjectVariant::Integer(1)),
                        ("W".to_string(), ObjectVariant::Integer(1)),
                    ])
                );
            }
            other => panic!("expected inline image, got {other:?}"),
        }
        assert!(matches!(
            result.last(),
            Some(PdfOperatorVariant::RestoreGraphicsState(
                RestoreGraphicsState
            ))
        ));
    }

    #[test]
    fn parsed_inline_image_is_recorded_by_backend() {
        let input = b"BI /W 1 /H 1 ID \x00 EI";
        let operators = PdfOperatorVariant::parse(input).unwrap();
        let inline_image = match operators.first() {
            Some(PdfOperatorVariant::InlineImage(image)) => image.clone(),
            other => panic!("expected inline image, got {other:?}"),
        };

        let mut backend = RecordingBackend::default();
        operators[0].call(&mut backend).unwrap();

        assert_eq!(
            backend.operations,
            vec![RecordedOperation::PaintInlineImage {
                image: inline_image
            }]
        );
    }

    #[test]
    fn parses_inline_image_with_exact_length_followed_by_newline_before_ei() {
        let input = b"46 0 0 0 1 1 d1\nq\n0 0 m\n0 1 l\n1 1 l\n1 0 l\nh\nW n\nq 1 0 0 -1 0 1 cm\nBI\n/IM true\n/W 1\n/H 1\n/BPC 1\n/D[1\n0]\nID \x00\nEI Q\nQ\n";

        let result = PdfOperatorVariant::parse(input).unwrap();

        assert!(matches!(
            result
                .iter()
                .find(|operator| matches!(operator, PdfOperatorVariant::InlineImage(_))),
            Some(PdfOperatorVariant::InlineImage(_))
        ));
    }

    #[test]
    fn test_simple() {
        struct TestCase<'a> {
            description: &'a str,
            input: &'a [u8],
            expected_ops: Vec<PdfOperatorVariant>,
        }

        let test_cases = vec![
            TestCase {
                description: "-1. Begin/End compatibility (BX/EX)",
                input: b"BX EX",
                expected_ops: vec![
                    PdfOperatorVariant::BeginCompatibility(BeginCompatibility),
                    PdfOperatorVariant::EndCompatibility(EndCompatibility),
                ],
            },
            TestCase {
                description: "0. ConcatMatrix(cm)",
                input: b"\n.17576218 0 0 .17576218 2227.4995 159.375 cm",
                expected_ops: vec![PdfOperatorVariant::ConcatMatrix(ConcatMatrix::new([
                    0.17576218, 0.0, 0.0, 0.17576218, 2227.4995, 159.375,
                ]))],
            },
            TestCase {
                description: "0b. Set flatness tolerance (i)",
                input: b"5 i",
                expected_ops: vec![PdfOperatorVariant::SetFlatnessTolerance(
                    SetFlatnessTolerance::new(5.0),
                )],
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
            TestCase {
                description: "25. Set non-stroking color (sc) with 3 components",
                input: b"0.1 0.2 0.3 sc",
                expected_ops: vec![PdfOperatorVariant::SetNonStrokingColor(
                    SetNonStrokingColor::new(vec![0.1, 0.2, 0.3], None),
                )],
            },
            TestCase {
                description: "26. Set non-stroking color (scn) with 3 components",
                input: b"0.972549 0.9764706 0.98039216 scn",
                expected_ops: vec![PdfOperatorVariant::SetNonStrokingColor(
                    SetNonStrokingColor::new(vec![0.972549, 0.9764706, 0.98039216], None),
                )],
            },
            TestCase {
                description: "27. Set rendering intent (ri)",
                input: b"/RelativeColorimetric ri",
                expected_ops: vec![PdfOperatorVariant::SetRenderingIntent(
                    SetRenderingIntent::new("RelativeColorimetric".to_string()),
                )],
            },
        ];

        for tc in test_cases {
            let actual_ops = PdfOperatorVariant::parse(tc.input).unwrap_or_else(|e| {
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

    #[test]
    fn parse_recovers_from_empty_name_operand_before_next_operator() {
        let input = b"/ 12 Tf q";

        let actual_ops = PdfOperatorVariant::parse(input).expect("stream should parse");

        assert_eq!(
            actual_ops,
            vec![
                PdfOperatorVariant::SetFont(SetFont::new(String::new(), 12.0)),
                PdfOperatorVariant::SaveGraphicsState(SaveGraphicsState),
            ]
        );
    }

    #[test]
    fn parse_handles_non_alphabetic_operator_keywords() {
        let input = b"1 2 d0 BT T* (x) ' 1 2 (y) \" ET";

        let actual_ops = PdfOperatorVariant::parse(input).expect("stream should parse");

        assert_eq!(actual_ops.len(), 6);
        assert!(matches!(
            actual_ops.first(),
            Some(PdfOperatorVariant::SetCharWidth(op)) if op.wx == 1.0
        ));
        assert!(matches!(
            actual_ops.get(1),
            Some(PdfOperatorVariant::BeginText(BeginText))
        ));
        assert!(matches!(
            actual_ops.get(2),
            Some(PdfOperatorVariant::MoveToNextLine(MoveToNextLine))
        ));
        assert!(matches!(
            actual_ops.get(3),
            Some(PdfOperatorVariant::MoveNextLineShowText(op)) if op == &MoveNextLineShowText::new(b"x".to_vec())
        ));
        assert!(matches!(
            actual_ops.get(4),
            Some(PdfOperatorVariant::SetSpacingMoveShowText(op))
                if op == &SetSpacingMoveShowText::new(1.0, 2.0, b"y".to_vec())
        ));
        assert!(matches!(
            actual_ops.get(5),
            Some(PdfOperatorVariant::EndText(EndText))
        ));
    }

    #[test]
    fn parse_skips_unknown_regular_character_token_and_recovers() {
        let input = b"@ q";

        let actual_ops = PdfOperatorVariant::parse(input).expect("stream should parse");

        assert_eq!(
            actual_ops,
            vec![PdfOperatorVariant::SaveGraphicsState(SaveGraphicsState)]
        );
    }

    #[test]
    fn parse_skips_malformed_operator_operand_sequence_and_recovers() {
        let input = b"1 2 3 m q";

        let actual_ops = PdfOperatorVariant::parse(input).expect("stream should parse");

        assert_eq!(
            actual_ops,
            vec![PdfOperatorVariant::SaveGraphicsState(SaveGraphicsState)]
        );
    }

    #[test]
    fn parse_recovers_from_truncated_trailing_array_operand() {
        let input = b"0 j 0 J [ ";

        let actual_ops = PdfOperatorVariant::parse(input).expect("stream should parse");

        assert_eq!(actual_ops.len(), 2);
        assert!(matches!(
            actual_ops.first(),
            Some(PdfOperatorVariant::SetLineJoinStyle(_))
        ));
        assert!(matches!(
            actual_ops.get(1),
            Some(PdfOperatorVariant::SetLineCapStyle(_))
        ));
    }

    #[test]
    fn parse_fixture_with_truncated_trailing_operand_stream() {
        let input = b"0 0 m 100 100 l 200 200 m 300 300 l S 1 j 0 J [ ";
        let actual_ops = PdfOperatorVariant::parse(input).expect("fixture stream should parse");

        assert!(!actual_ops.is_empty());
    }
}
