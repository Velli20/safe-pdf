use crate::{
    clipping_path_operators::*,
    color_operators::*,
    compatibility_operators::*,
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
    type3_font_operators::{SetCharWidth, SetCharWidthAndBoundingBox},
    xobject_and_image_operators::*,
};

use pdf_parser::parser::PdfParser;

/// Defines a mapping between a PDF operator's string representation (e.g., "m" for MoveTo)
/// and a function that can construct that operator an array of operands.
/// This is used to dynamically dispatch to the correct parsing logic based on the operator
/// encountered in the PDF content.
pub struct OpDescriptor {
    pub name: &'static [u8],
    pub operand_count: Option<usize>,
    pub parser: fn(operands: &mut Operands) -> Result<PdfOperatorVariant, PdfOperatorError>,
    pub parse_hook: for<'a> fn(&'a mut PdfParser<'a>) -> Result<Option<PdfOperatorVariant>, PdfOperatorError>,
}

impl OpDescriptor {
    const fn from<T: PdfOperator>() -> Self {
        Self {
            name: T::NAME,
            operand_count: T::OPERAND_COUNT,
            parser: T::read,
            parse_hook: T::parse,
        }
    }
}

// MUST remain sorted lexicographically by `name` — binary search depends on this.
// If you add a new entry, insert it in the correct position and verify with the
// `read_map_is_sorted` test below.
pub(crate) const READ_MAP: &[OpDescriptor] = &[
    OpDescriptor::from::<SetSpacingMoveShowText>(), // "\""
    OpDescriptor::from::<MoveNextLineShowText>(),   // "'"
    OpDescriptor::from::<FillAndStrokePathNonZero>(), // "B"
    OpDescriptor::from::<FillAndStrokePathEvenOdd>(), // "B*"
    OpDescriptor::from::<BeginMarkedContentWithProps>(), // "BDC"
    OpDescriptor::from::<pdf_image::InlineImage>(), // "BI"
    OpDescriptor::from::<BeginMarkedContent>(),     // "BMC"
    OpDescriptor::from::<BeginText>(),              // "BT"
    OpDescriptor::from::<BeginCompatibility>(),     // "BX"
    OpDescriptor::from::<SetStrokeColorSpace>(),    // "CS"
    OpDescriptor::from::<InvokeXObject>(),          // "Do"
    OpDescriptor::from::<EndMarkedContent>(),       // "EMC"
    OpDescriptor::from::<EndText>(),                // "ET"
    OpDescriptor::from::<EndCompatibility>(),       // "EX"
    OpDescriptor::from::<SetGrayStroke>(),          // "G"
    OpDescriptor::from::<SetLineCapStyle>(),        // "J"
    OpDescriptor::from::<SetCMYKStroke>(),          // "K"
    OpDescriptor::from::<SetMiterLimit>(),          // "M"
    OpDescriptor::from::<RestoreGraphicsState>(),   // "Q"
    OpDescriptor::from::<SetRGBStroke>(),           // "RG"
    OpDescriptor::from::<StrokePath>(),             // "S"
    OpDescriptor::from::<SetStrokingColorSc>(),     // "SC"
    OpDescriptor::from::<SetStrokingColor>(),       // "SCN"
    OpDescriptor::from::<MoveToNextLine>(),         // "T*"
    OpDescriptor::from::<MoveTextPositionAndSetLeading>(), // "TD"
    OpDescriptor::from::<ShowTextArray>(),          // "TJ"
    OpDescriptor::from::<SetLeading>(),             // "TL"
    OpDescriptor::from::<SetCharacterSpacing>(),    // "Tc"
    OpDescriptor::from::<MoveTextPosition>(),       // "Td"
    OpDescriptor::from::<SetFont>(),                // "Tf"
    OpDescriptor::from::<ShowText>(),               // "Tj"
    OpDescriptor::from::<SetTextMatrix>(),          // "Tm"
    OpDescriptor::from::<SetRenderingMode>(),       // "Tr"
    OpDescriptor::from::<SetTextRise>(),            // "Ts"
    OpDescriptor::from::<SetWordSpacing>(),         // "Tw"
    OpDescriptor::from::<SetHorizontalScaling>(),   // "Tz"
    OpDescriptor::from::<ClipNonZero>(),            // "W"
    OpDescriptor::from::<ClipEvenOdd>(),            // "W*"
    OpDescriptor::from::<CloseFillAndStrokePathNonZero>(), // "b"
    OpDescriptor::from::<CloseFillAndStrokePathEvenOdd>(), // "b*"
    OpDescriptor::from::<CurveTo>(),                // "c"
    OpDescriptor::from::<ConcatMatrix>(),           // "cm"
    OpDescriptor::from::<SetNonStrokingColorSpace>(), // "cs"
    OpDescriptor::from::<SetDashPattern>(),         // "d"
    OpDescriptor::from::<SetCharWidth>(),           // "d0"
    OpDescriptor::from::<SetCharWidthAndBoundingBox>(), // "d1"
    OpDescriptor::from::<FillPathNonZero>(),        // "f"
    OpDescriptor::from::<FillPathEvenOdd>(),        // "f*"
    OpDescriptor::from::<SetGrayFill>(),            // "g"
    OpDescriptor::from::<SetGraphicsStateFromDict>(), // "gs"
    OpDescriptor::from::<ClosePath>(),              // "h"
    OpDescriptor::from::<SetFlatnessTolerance>(),   // "i"
    OpDescriptor::from::<SetLineJoinStyle>(),       // "j"
    OpDescriptor::from::<SetCMYKFill>(),            // "k"
    OpDescriptor::from::<LineTo>(),                 // "l"
    OpDescriptor::from::<MoveTo>(),                 // "m"
    OpDescriptor::from::<EndPath>(),                // "n"
    OpDescriptor::from::<SaveGraphicsState>(),      // "q"
    OpDescriptor::from::<Rectangle>(),              // "re"
    OpDescriptor::from::<SetRGBFill>(),             // "rg"
    OpDescriptor::from::<SetRenderingIntent>(),     // "ri"
    OpDescriptor::from::<CloseStrokePath>(),        // "s"
    OpDescriptor::from::<SetNonStrokingColorSc>(),  // "sc"
    OpDescriptor::from::<SetNonStrokingColor>(),    // "scn"
    OpDescriptor::from::<PaintShading>(),           // "sh"
    OpDescriptor::from::<CurveToV>(),               // "v"
    OpDescriptor::from::<SetLineWidth>(),           // "w"
    OpDescriptor::from::<CurveToY>(),               // "y"
];

pub fn get_operation_descriptor(name: &[u8]) -> Option<&'static OpDescriptor> {
    READ_MAP
        .binary_search_by(|op| op.name.cmp(name))
        .ok()
        .and_then(|idx| READ_MAP.get(idx))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn read_map_is_sorted() {
        let names: Vec<_> = READ_MAP.iter().map(|op| op.name).collect();
        let mut expected = names.clone();
        expected.sort_unstable();
        assert_eq!(
            names, expected,
            "READ_MAP must be sorted lexicographically by name for binary search to work"
        );
    }
}
