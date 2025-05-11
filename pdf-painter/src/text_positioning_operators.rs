use crate::PdfOperator;

/// Moves to the start of the next line, offset from the start of the current line by (`tx`, `ty`). (PDF operator `Td`)
/// `tx` and `ty` are numbers expressed in unscaled text space units.
/// More precisely, this operator sets the text line matrix, T_lm, to:
/// `[1 0 0 1 tx ty] * T_lm`
#[derive(Debug, Clone, PartialEq)]
pub struct MoveTextPosition {
    /// The horizontal offset.
    tx: f32,
    /// The vertical offset.
    ty: f32,
}

impl PdfOperator for MoveTextPosition {
    fn operator() -> &'static str {
        "Td"
    }
}

impl MoveTextPosition {
    pub fn new(tx: f32, ty: f32) -> Self {
        Self { tx, ty }
    }
}

/// Moves to the start of the next line, offset from the start of the current line by (`tx`, `ty`),
/// and sets the text leading, `TL`, to `-ty`. (PDF operator `TD`)
/// This has the same effect as `SetLeading { leading: -ty }` followed by `MoveTextPosition { tx, ty }`.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveTextPositionAndSetLeading {
    /// The horizontal offset.
    tx: f32,
    /// The vertical offset. The new text leading will be set to the negative of this value.
    ty: f32,
}

impl PdfOperator for MoveTextPositionAndSetLeading {
    fn operator() -> &'static str {
        "TD"
    }
}

impl MoveTextPositionAndSetLeading {
    pub fn new(tx: f32, ty: f32) -> Self {
        Self { tx, ty }
    }
}

/// Sets the text matrix, `Tm`, and the text line matrix, `Tlm`. (PDF operator `Tm`)
/// The matrix is specified in the form `[a b c d e f]`.
#[derive(Debug, Clone, PartialEq)]
pub struct SetTextMatrix {
    /// The 6-element array representing the text matrix.
    /// `[a, b, c, d, e, f]` corresponds to the matrix:
    /// `a b 0`
    /// `c d 0`
    /// `e f 1`
    matrix: [f32; 6],
}

impl PdfOperator for SetTextMatrix {
    fn operator() -> &'static str {
        "Tm"
    }
}

impl SetTextMatrix {
    pub fn new(matrix: [f32; 6]) -> Self {
        Self { matrix }
    }
}

/// Moves to the start of the next line. (PDF operator `T*`)
/// This has the same effect as `MoveTextPosition { tx: 0.0, ty: -leading }`,
/// where `leading` is the current value of the text leading parameter in the text state.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveToNextLine;

impl PdfOperator for MoveToNextLine {
    fn operator() -> &'static str {
        "T*"
    }
}

impl MoveToNextLine {
    pub fn new() -> Self {
        Self
    }
}
