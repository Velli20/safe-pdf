use std::sync::Arc;

use pdf_graphics::{LineCap, LineJoin, TextRenderingMode, transform::Transform};
use pdf_image::PdfImage;
use pdf_object_reader::{ReadResult, pdf_string::PdfString};

use crate::{
    OperandContext, PdfTextItem, color_operators::ColorOperands,
    marked_content_operators::BeginMarkedContentWithProps,
};

/// One PDF content-stream operation with its decoded operands.
#[derive(Debug, Clone)]
pub enum PdfOperatorVariant {
    /// Appends a straight line segment from the current point to the specified point (x, y).
    /// The new current point becomes (x, y).
    LineTo {
        /// The x-coordinate of the line segment's end point.
        x: f32,
        /// The y-coordinate of the line segment's end point.
        y: f32,
    },
    /// Begins a new subpath by moving the current point to coordinates (x, y), omitting any connecting line segment.
    /// If the `m` operator is the first operator in a path, it sets the current point.
    MoveTo {
        /// The x-coordinate of the new current point.
        x: f32,
        /// The y-coordinate of the new current point.
        y: f32,
    },
    /// Appends a cubic Bézier curve to the current path.
    /// The curve extends from the current point to (x3, y3), using (x1, y1) and (x2, y2) as Bézier control points.
    /// The new current point becomes (x3, y3).
    CurveTo {
        /// The x-coordinate of the first Bézier control point.
        x1: f32,
        /// The y-coordinate of the first Bézier control point.
        y1: f32,
        /// The x-coordinate of the second Bézier control point.
        x2: f32,
        /// The y-coordinate of the second Bézier control point.
        y2: f32,
        /// The x-coordinate of the curve's end point.
        x3: f32,
        /// The y-coordinate of the curve's end point.
        y3: f32,
    },
    /// Appends a cubic Bézier curve to the current path.
    /// The current point is used as the first control point (x1, y1).
    /// (x2, y2) is the second Bézier control point, and (x3, y3) is the end point of the curve.
    /// The new current point becomes (x3, y3).
    CurveToV {
        /// The x-coordinate of the second Bézier control point.
        x2: f32,
        /// The y-coordinate of the second Bézier control point.
        y2: f32,
        /// The x-coordinate of the curve's end point.
        x3: f32,
        /// The y-coordinate of the curve's end point.
        y3: f32,
    },
    /// Appends a cubic Bézier curve to the current path.
    /// (x1, y1) is the first Bézier control point. The second control point (x2, y2) is the same as the curve's end point (x3, y3).
    /// The new current point becomes (x3, y3).
    CurveToY {
        /// The x-coordinate of the first Bézier control point.
        x1: f32,
        /// The y-coordinate of the first Bézier control point.
        y1: f32,
        /// The x-coordinate of the curve's end point (and second control point).
        x3: f32,
        /// The y-coordinate of the curve's end point (and second control point).
        y3: f32,
    },
    /// Closes the current subpath by appending a straight line segment from the current point
    /// to the starting point of the subpath. (PDF operator `h`)
    ClosePath,
    /// Appends a complete rectangle, defined by its bottom-left corner (x, y), width, and height,
    /// to the current path as a complete subpath.
    /// The new current point is undefined after this operation.
    Rectangle {
        /// The x-coordinate of the bottom-left corner of the rectangle.
        x: f32,
        /// The y-coordinate of the bottom-left corner of the rectangle.
        y: f32,
        /// The width of the rectangle.
        width: f32,
        /// The height of the rectangle.
        height: f32,
    },
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
    /// Sets the fill color to a grayscale value.
    /// The gray level applies to subsequent fill operations.
    SetGrayFill {
        /// The gray level, a value between 0.0 (black) and 1.0 (white).
        gray: f32,
    },
    /// Sets the stroke color to a grayscale value.
    /// The gray level applies to subsequent stroke operations.
    SetGrayStroke {
        /// The gray level, a value between 0.0 (black) and 1.0 (white).
        gray: f32,
    },
    /// Sets the fill color to an RGB (Red, Green, Blue) value.
    /// The RGB color applies to subsequent fill operations.
    SetRGBFill {
        /// The red component, a value between 0.0 and 1.0.
        r: f32,
        /// The green component, a value between 0.0 and 1.0.
        g: f32,
        /// The blue component, a value between 0.0 and 1.0.
        b: f32,
    },
    /// Sets the stroke color to an RGB (Red, Green, Blue) value.
    /// The RGB color applies to subsequent stroke operations.
    SetRGBStroke {
        /// The red component, a value between 0.0 and 1.0.
        r: f32,
        /// The green component, a value between 0.0 and 1.0.
        g: f32,
        /// The blue component, a value between 0.0 and 1.0.
        b: f32,
    },
    /// Sets the fill color to a CMYK (Cyan, Magenta, Yellow, Black/Key) value.
    /// The CMYK color applies to subsequent fill operations.
    SetCMYKFill {
        /// The cyan component, a value between 0.0 and 1.0.
        c: f32,
        /// The magenta component, a value between 0.0 and 1.0.
        m: f32,
        /// The yellow component, a value between 0.0 and 1.0.
        y: f32,
        /// The black (key) component, a value between 0.0 and 1.0.
        k: f32,
    },
    /// Sets the stroke color to a CMYK (Cyan, Magenta, Yellow, Black/Key) value.
    /// The CMYK color applies to subsequent stroke operations.
    SetCMYKStroke {
        /// The cyan component, a value between 0.0 and 1.0.
        c: f32,
        /// The magenta component, a value between 0.0 and 1.0.
        m: f32,
        /// The yellow component, a value between 0.0 and 1.0.
        y: f32,
        /// The black (key) component, a value between 0.0 and 1.0.
        k: f32,
    },
    /// Sets the line width for path stroking.
    SetLineWidth {
        /// The new line width in user space units.
        width: f32,
    },
    /// Sets the line cap style for path stroking.
    SetLineCapStyle {
        /// The line cap style to apply.
        style: LineCap,
    },
    /// Sets the line join style for path stroking.
    SetLineJoinStyle {
        /// The line join style to apply.
        style: LineJoin,
    },
    /// Sets the miter limit for path stroking.
    SetMiterLimit {
        /// The new miter limit. This controls when a miter join is automatically
        /// converted to a bevel join to prevent excessively long spikes.
        limit: f32,
    },
    /// Sets the dash pattern for path stroking.
    SetDashPattern {
        /// An array of numbers specifying the lengths of alternating dashes and gaps.
        array: Vec<f32>,
        /// The phase, specifying the distance into the dash pattern at which to start.
        phase: f32,
    },
    /// Sets the flatness tolerance, controlling the accuracy with which curves
    /// are approximated. (PDF operator `i`)
    SetFlatnessTolerance {
        /// The flatness tolerance value. Typical values range from 1 to 100.
        tolerance: f32,
    },
    /// Sets multiple graphics state parameters from a named graphics state parameter dictionary.
    /// The dictionary is expected to be in the resource dictionary. (PDF operator `gs`)
    SetGraphicsStateFromDict {
        /// The name of the graphics state parameter dictionary.
        dict_name: Arc<[u8]>,
    },
    /// Saves the current graphics state on the graphics state stack.
    SaveGraphicsState,
    /// Restores the graphics state by removing the most recently saved state from the stack.
    RestoreGraphicsState,
    /// Modifies the current transformation matrix (CTM) by concatenating the specified matrix.
    ConcatMatrix {
        /// The matrix to concatenate with the CTM.
        matrix: Transform,
    },
    /// Begins a marked-content sequence.
    /// Marked-content sequences associate a tag with a sequence of content stream operations.
    BeginMarkedContent {
        /// The tag indicating the role or nature of the marked-content sequence.
        tag: Arc<[u8]>,
    },
    BeginMarkedContentWithProps(BeginMarkedContentWithProps),
    /// Ends a marked-content sequence begun by a `BMC` or `BDC` operator.
    EndMarkedContent,
    /// Begins a text object, initializing the text matrix and text line matrix to
    /// the identity matrix.
    BeginText,
    /// Ends a text object, discarding the text matrix and text line matrix.
    EndText,
    /// Moves to the start of the next line, offset from the start of the current line by (`tx`, `ty`).
    /// `tx` and `ty` are numbers expressed in unscaled text space units.
    /// More precisely, this operator sets the text line matrix, T_lm, to:
    /// `[1 0 0 1 tx ty] * T_lm`
    MoveTextPosition {
        /// The horizontal offset.
        tx: f32,
        /// The vertical offset.
        ty: f32,
    },
    /// Moves to the start of the next line, offset from the start of the current line by (`tx`, `ty`),
    /// and sets the text leading, `TL`, to `-ty`. (PDF operator `TD`)
    /// This has the same effect as `SetLeading { leading: -ty }` followed by `MoveTextPosition { tx, ty }`.
    MoveTextPositionAndSetLeading {
        /// The horizontal offset.
        tx: f32,
        /// The vertical offset. The new text leading will be set to the negative of this value.
        ty: f32,
    },
    /// Sets the text matrix, `Tm`, and the text line matrix, `Tlm`.
    SetTextMatrix {
        /// The text matrix.
        matrix: Transform,
    },
    /// Moves to the start of the next line.
    /// This has the same effect as `MoveTextPosition { tx: 0.0, ty: -leading }`,
    /// where `leading` is the current value of the text leading parameter in the text state.
    MoveToNextLine,
    /// Shows a text string.
    ShowText {
        /// The text item to be shown. Its bytes are typically encoded according to the font's
        /// encoding.
        text: PdfTextItem,
    },
    /// Moves to the next line and shows a text string.
    MoveNextLineShowText {
        /// The text string to be shown.
        text: PdfTextItem,
    },
    /// Sets the word and character spacing, moves to the next line, and shows a text string.
    /// This is equivalent to `SetWordSpacing`, `SetCharacterSpacing`, and `MoveNextLineShowText`.
    SetSpacingMoveShowText {
        /// The new word spacing to set before showing the text.
        word_spacing: f32,
        /// The new character spacing to set before showing the text.
        char_spacing: f32,
        /// The text string to be shown.
        text: PdfTextItem,
    },
    /// Shows one or more text strings, allowing individual glyph positioning.
    /// The array can contain strings and numbers. Numbers specify an additional horizontal or vertical
    /// displacement (depending on the writing mode) to apply before showing the next string or glyph.
    ShowTextArray {
        /// A vector of `PdfTextItem`s, where each item is either a string to show
        /// or a numeric adjustment for positioning.
        elements: Vec<PdfTextItem>,
    },
    /// Sets the character spacing, `Tc`, which is a number expressed in unscaled text space units.
    SetCharacterSpacing {
        /// The character spacing. Added to the horizontal displacement otherwise produced by showing a glyph.
        spacing: f32,
    },
    /// Sets the word spacing, `Tw`, which is a number expressed in unscaled text space units.
    /// Word spacing is used by the `Tj`, `'`, and `"` operators.
    SetWordSpacing {
        /// The word spacing. Added to the character spacing when the character is a space (char code 32).
        spacing: f32,
    },
    /// Sets the horizontal scaling, `Tz`, which adjusts the width of glyphs by
    /// stretching or compressing them horizontally.
    SetHorizontalScaling {
        /// The horizontal scaling factor as a percentage (e.g., 100.0 for 100% - no scaling).
        scale: f32,
    },
    /// Sets the text leading, `TL`, which is the vertical distance between the baselines of
    /// adjacent lines of text.
    SetLeading {
        /// The text leading, in unscaled text space units.
        leading: f32,
    },
    /// Sets the text font, `Tf`, to a font resource in the resource dictionary and the text
    /// font size, `Tfs`, in unscaled text space units.
    SetFont {
        /// The name of the font resource.
        name: Arc<[u8]>,
        /// The font size.
        size: f32,
    },
    /// Sets the text rendering mode, which determines whether text is filled, stroked,
    /// used as a clipping path, or some combination.
    SetRenderingMode {
        /// The rendering mode.
        mode: TextRenderingMode,
    },
    /// Sets the text rise, `Ts`, which specifies the vertical distance
    /// to shift the baseline of text relative to the current baseline.
    SetTextRise {
        /// The text rise, in unscaled text space units. A positive value moves the baseline up.
        rise: f32,
    },
    /// Invokes a named XObject.
    /// XObjects are external objects such as images or self-contained page descriptions (Form XObjects).
    InvokeXObject {
        /// The name of the XObject resource to invoke, as defined in the resource dictionary.
        name: Arc<[u8]>,
    },
    /// Begins a compatibility section (BX).
    ///
    /// According to the PDF specification, a compatibility section allows
    /// consumers to ignore any operators they do not recognize until the
    /// matching EX operator is encountered. For our purposes, we parse and
    /// expose these operators so unknown operators within can be tolerated
    /// by higher-level logic. Backends may choose to track nesting depth,
    /// but by default these operators are no-ops.
    BeginCompatibility,
    InlineImage(Arc<PdfImage>),
    /// Ends a compatibility section (EX).
    EndCompatibility,
    /// Paints the shape and color shading defined by a shading dictionary resource.
    /// The `sh` operator takes one operand, the name of a shading dictionary.
    PaintShading {
        /// The name of the shading dictionary resource from the Shading subdictionary
        /// of the current resource dictionary.
        name: Arc<[u8]>,
    },
    SetCharWidthAndBoundingBox {
        /// The x-component of the character width vector.
        wx: f32,
        /// The y-component of the character width vector.
        wy: f32,
        /// The x-coordinate of the lower-left corner of the character bounding box.
        llx: f32,
        /// The y-coordinate of the lower-left corner of the character bounding box.
        lly: f32,
        /// The x-coordinate of the upper-right corner of the character bounding box.
        urx: f32,
        /// The y-coordinate of the upper-right corner of the character bounding box.
        ury: f32,
    },
    SetCharWidth {
        /// The x-component of the character width vector.
        wx: f32,
        /// The y-component of the character width vector.
        wy: f32,
    },
    /// Sets the rendering intent for color reproduction. (PDF operator `ri`)
    ///
    /// The operand is a name object indicating the intent, e.g.,
    /// /AbsoluteColorimetric, /RelativeColorimetric, /Saturation, /Perceptual.
    SetRenderingIntent {
        intent: Arc<[u8]>,
    },
    /// Sets the stroke color space value.
    SetStrokeColorSpace {
        /// The name of the color space.
        name: Arc<[u8]>,
    },
    /// Sets the non-stroking (fill) color space value.
    SetNonStrokingColorSpace {
        /// The name of the color space.
        name: Arc<[u8]>,
    },
    /// Sets the stroking color when the color space requires
    /// multiple color components.
    SetStrokingColor(ColorOperands),
    /// Sets the non-stroking color when the color space requires
    /// multiple color components.
    SetNonStrokingColor(ColorOperands),
}

impl PdfOperatorVariant {
    /// Reads the operator named `name` from the operands collected before it.
    ///
    /// Returns `Ok(None)` for an unknown operator or one given the wrong number
    /// of operands. Inline images (`BI`) carry their own grammar and are parsed
    /// from the content stream instead.
    pub fn read(name: &[u8], operands: &mut OperandContext<'_, '_>) -> ReadResult<Option<Self>> {
        let operator = match (name, operands.array().len()) {
            (b"\"", 3) => Self::SetSpacingMoveShowText {
                word_spacing: operands.at(0)?,
                char_spacing: operands.at(1)?,
                text: PdfTextItem::Text(operands.at::<PdfString>(2)?.bytes),
            },
            (b"'", 1) => Self::MoveNextLineShowText {
                text: PdfTextItem::Text(operands.at::<PdfString>(0)?.bytes),
            },
            (b"B", 0) => Self::FillAndStrokePathNonZero,
            (b"B*", 0) => Self::FillAndStrokePathEvenOdd,
            (b"BDC", 2) => {
                Self::BeginMarkedContentWithProps(BeginMarkedContentWithProps::read(operands)?)
            }
            (b"BMC", 1) => Self::BeginMarkedContent {
                tag: operands.at::<PdfString>(0)?.bytes,
            },
            (b"BT", 0) => Self::BeginText,
            (b"BX", 0) => Self::BeginCompatibility,
            (b"CS", 1) => Self::SetStrokeColorSpace {
                name: operands.at::<PdfString>(0)?.bytes,
            },
            (b"Do", 1) => Self::InvokeXObject {
                name: operands.at::<PdfString>(0)?.bytes,
            },
            (b"EMC", 0) => Self::EndMarkedContent,
            (b"ET", 0) => Self::EndText,
            (b"EX", 0) => Self::EndCompatibility,
            (b"G", 1) => Self::SetGrayStroke {
                gray: operands.at(0)?,
            },
            (b"J", 1) => Self::SetLineCapStyle {
                style: operands.at(0)?,
            },
            (b"K", 4) => operands
                .read_array()
                .map(|[c, m, y, k]| Self::SetCMYKStroke { c, m, y, k })?,
            (b"M", 1) => Self::SetMiterLimit {
                limit: operands.at(0)?,
            },
            (b"Q", 0) => Self::RestoreGraphicsState,
            (b"RG", 3) => operands
                .read_array()
                .map(|[r, g, b]| Self::SetRGBStroke { r, g, b })?,
            (b"S", 0) => Self::StrokePath,
            (b"SC", _) => {
                Self::SetStrokingColor(ColorOperands::read_without_pattern(operands, "SC")?)
            }
            (b"SCN", _) => Self::SetStrokingColor(ColorOperands::read(operands)?),
            (b"T*", 0) => Self::MoveToNextLine,
            (b"TD", 2) => operands
                .read_array()
                .map(|[tx, ty]| Self::MoveTextPositionAndSetLeading { tx, ty })?,
            (b"TJ", 1) => Self::ShowTextArray {
                elements: operands.at(0)?,
            },
            (b"TL", 1) => Self::SetLeading {
                leading: operands.at(0)?,
            },
            (b"Tc", 1) => Self::SetCharacterSpacing {
                spacing: operands.at(0)?,
            },
            (b"Td", 2) => operands
                .read_array()
                .map(|[tx, ty]| Self::MoveTextPosition { tx, ty })?,
            (b"Tf", 2) => Self::SetFont {
                name: operands.at::<PdfString>(0)?.bytes,
                size: operands.at(1)?,
            },
            (b"Tj", 1) => Self::ShowText {
                text: PdfTextItem::Text(operands.at::<PdfString>(0)?.bytes),
            },
            (b"Tm", 6) => Self::SetTextMatrix {
                matrix: Transform::from(operands.read_array::<f32, 6>()?),
            },
            (b"Tr", 1) => Self::SetRenderingMode {
                mode: operands.at(0)?,
            },
            (b"Ts", 1) => Self::SetTextRise {
                rise: operands.at(0)?,
            },
            (b"Tw", 1) => Self::SetWordSpacing {
                spacing: operands.at(0)?,
            },
            (b"Tz", 1) => Self::SetHorizontalScaling {
                scale: operands.at(0)?,
            },
            (b"W", 0) => Self::ClipNonZero,
            (b"W*", 0) => Self::ClipEvenOdd,
            (b"b", 0) => Self::CloseFillAndStrokePathNonZero,
            (b"b*", 0) => Self::CloseFillAndStrokePathEvenOdd,
            (b"c", 6) => operands
                .read_array()
                .map(|[x1, y1, x2, y2, x3, y3]| Self::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                })?,
            (b"cm", 6) => Self::ConcatMatrix {
                matrix: Transform::from(operands.read_array::<f32, 6>()?),
            },
            (b"cs", 1) => Self::SetNonStrokingColorSpace {
                name: operands.at::<PdfString>(0)?.bytes,
            },
            (b"d", 2) => Self::SetDashPattern {
                array: operands.at(0)?,
                phase: operands.at(1)?,
            },
            (b"d0", 2) => operands
                .read_array()
                .map(|[wx, wy]| Self::SetCharWidth { wx, wy })?,
            (b"d1", 6) => operands.read_array().map(|[wx, wy, llx, lly, urx, ury]| {
                Self::SetCharWidthAndBoundingBox {
                    wx,
                    wy,
                    llx,
                    lly,
                    urx,
                    ury,
                }
            })?,
            (b"f", 0) => Self::FillPathNonZero,
            (b"f*", 0) => Self::FillPathEvenOdd,
            (b"g", 1) => Self::SetGrayFill {
                gray: operands.at(0)?,
            },
            (b"gs", 1) => Self::SetGraphicsStateFromDict {
                dict_name: operands.at::<PdfString>(0)?.bytes,
            },
            (b"h", 0) => Self::ClosePath,
            (b"i", 1) => Self::SetFlatnessTolerance {
                tolerance: operands.at(0)?,
            },
            (b"j", 1) => Self::SetLineJoinStyle {
                style: operands.at(0)?,
            },
            (b"k", 4) => operands
                .read_array()
                .map(|[c, m, y, k]| Self::SetCMYKFill { c, m, y, k })?,
            (b"l", 2) => operands.read_array().map(|[x, y]| Self::LineTo { x, y })?,
            (b"m", 2) => operands.read_array().map(|[x, y]| Self::MoveTo { x, y })?,
            (b"n", 0) => Self::EndPath,
            (b"q", 0) => Self::SaveGraphicsState,
            (b"re", 4) => operands
                .read_array()
                .map(|[x, y, width, height]| Self::Rectangle {
                    x,
                    y,
                    width,
                    height,
                })?,
            (b"rg", 3) => operands
                .read_array()
                .map(|[r, g, b]| Self::SetRGBFill { r, g, b })?,
            (b"ri", 1) => Self::SetRenderingIntent {
                intent: operands.at::<PdfString>(0)?.bytes,
            },
            (b"s", 0) => Self::CloseStrokePath,
            (b"sc", _) => {
                Self::SetNonStrokingColor(ColorOperands::read_without_pattern(operands, "sc")?)
            }
            (b"scn", _) => Self::SetNonStrokingColor(ColorOperands::read(operands)?),
            (b"sh", 1) => Self::PaintShading {
                name: operands.at::<PdfString>(0)?.bytes,
            },
            (b"v", 4) => operands
                .read_array()
                .map(|[x2, y2, x3, y3]| Self::CurveToV { x2, y2, x3, y3 })?,
            (b"w", 1) => Self::SetLineWidth {
                width: operands.at(0)?,
            },
            (b"y", 4) => operands
                .read_array()
                .map(|[x1, y1, x3, y3]| Self::CurveToY { x1, y1, x3, y3 })?,
            _ => return Ok(None),
        };
        Ok(Some(operator))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use pdf_object_reader::{
        ArrayContext, ObjectReader, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant,
    };

    use super::*;

    #[test]
    fn show_text_moves_the_operand_byte_allocation() {
        let text: Arc<[u8]> = Arc::from(b"text without a copy".to_vec());

        let original_pointer = text.as_ptr();

        let operands = [ObjectVariant::String(
            pdf_object_reader::pdf_string::PdfString {
                bytes: text,
                kind: pdf_object_reader::string_kind::StringKind::Literal,
            },
        )];
        let reader = ObjectReader::new(PassthroughResolver);
        let mut session = reader.session();

        let operator =
            PdfOperatorVariant::read(b"Tj", &mut ArrayContext::new(&operands, &mut session))
                .expect("text should parse");
        let Some(PdfOperatorVariant::ShowText {
            text: PdfTextItem::Text(text),
        }) = operator
        else {
            panic!("expected a ShowText text item");
        };
        assert_eq!(&*text, b"text without a copy");
        assert_eq!(text.as_ptr(), original_pointer);
    }

    #[test]
    fn spacing_move_show_text_groups_numeric_prefix_before_text() {
        let operands = [
            ObjectVariant::Real(1.5),
            ObjectVariant::Integer(2),
            pdf_object_reader::pdf_string::PdfString::from(
                b"text",
                pdf_object_reader::string_kind::StringKind::Literal,
            ),
        ];
        let reader = ObjectReader::new(PassthroughResolver);
        let mut session = reader.session();

        let operator =
            PdfOperatorVariant::read(b"\"", &mut ArrayContext::new(&operands, &mut session))
                .expect("mixed operands should parse successfully");
        let Some(PdfOperatorVariant::SetSpacingMoveShowText {
            word_spacing,
            char_spacing,
            text,
        }) = operator
        else {
            panic!("expected SetSpacingMoveShowText");
        };
        assert_eq!(word_spacing, 1.5);
        assert_eq!(char_spacing, 2.0);
        assert_eq!(text, PdfTextItem::Text(Arc::from(&b"text"[..])));
    }
}
