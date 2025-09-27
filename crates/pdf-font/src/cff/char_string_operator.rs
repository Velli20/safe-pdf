//! CFF Type 2 CharString operator metadata.
//!
//! This module defines a zero-sized struct for each CharString operator. Each struct exposes:
//! - `OPCODE`: the encoded operator value as `u16` (two-byte ops use `12 << 8 | n`).
//! - `MIN_OPERANDS` and `MAX_OPERANDS`: the number of stack operands consumed.
//!
//! Notes
//! - Some drawing operators are variadic; in those cases `MAX_OPERANDS` is `usize::MAX` and
//!   operands are consumed in well-defined groups (for example, `rlineto` uses groups of 2).
//! - The special-case width operand in CharStrings is handled by the caller and not counted here.
//! - Hint mask operators also read mask bytes from the stream; only stack operands are counted.

use pdf_graphics::{pdf_path::PdfPath, point::Point};

use thiserror::Error;

use crate::cff::{cursor::Cursor, error::CompactFontFormatError, parser::parse_int};

/// Specifies how point coordinates for a curve are computed.
#[derive(Copy, Clone)]
enum PointMode {
    DxDy,
    XDy,
    DxY,
    DxMaybeDy(bool),
    MaybeDxDy(bool),
}

#[derive(Default)]
pub struct CharStringStack {
    pub operands: Vec<i32>,
    is_open: bool,
    have_read_width: bool,
    x: f32,
    y: f32,
    stack_ix: usize,
}

impl CharStringStack {
    pub fn push(&mut self, v: i32) {
        self.operands.push(v);
    }

    pub fn pop(&mut self) -> Option<i32> {
        self.operands.pop()
    }

    pub fn len(&self) -> usize {
        self.operands.len()
    }

    pub fn clear(&mut self) {
        self.operands.clear();
        self.stack_ix = 0;
    }

    pub fn get_fixed(&self, index: usize) -> Result<f32, CompactFontFormatError> {
        if index >= self.operands.len() {
            return Err(CompactFontFormatError::StackUnderflow);
        }
        Ok(self.operands[index] as f32)
    }

    fn coords_remaining(&self) -> usize {
        // This is overly defensive to avoid overflow but in the case of
        // broken fonts, just return 0 when stack_ix > stack_len to prevent
        // potential runaway while loops in the evaluator if this wraps
        self.operands.len().saturating_sub(self.stack_ix)
    }

    pub fn fixed_array<const N: usize>(
        &self,
        first_index: usize,
    ) -> Result<[f32; N], CompactFontFormatError> {
        if first_index + N > self.operands.len() {
            return Err(CompactFontFormatError::StackUnderflow);
        }
        let mut arr = [0.0; N];
        for i in 0..N {
            arr[i] = self.operands[first_index + i] as f32;
        }
        Ok(arr)
    }

    /// Returns true if the number of elements on the stack is odd.
    ///
    /// Used for processing some charstring operators where an odd
    /// count represents the presence of the glyph advance width at the
    /// bottom of the stack.
    pub fn len_is_odd(&self) -> bool {
        self.operands.len() % 2 == 1
    }
}

// The arithmetic in this helper is limited to small index/offset adjustments based on
// the operand stack pointer. These additions are safe in practice because the stack
// length is bounded by the charstring operand encoding (far below usize::MAX). We
// suppress Clippy's `arithmetic_side_effects` here to keep the code concise.
#[allow(clippy::arithmetic_side_effects)]
fn emit_curves<const N: usize>(
    stack: &mut CharStringStack,
    path: &mut PdfPath,
    modes: [PointMode; N],
) -> Result<(), CompactFontFormatError> {
    // In Type2 charstrings this helper is always used with 3 coordinate specs (two control
    // points + final on-curve point). We keep it generic for clarity, but assert in debug
    // builds to catch accidental misuse.
    debug_assert!(
        N == 3,
        "emit_curves is expected to be called with exactly 3 point modes"
    );

    let mut control_points = [Point::default(); 2];

    for (i, mode) in modes.into_iter().enumerate() {
        let used = match mode {
            PointMode::DxDy => {
                stack.x += stack.get_fixed(stack.stack_ix)?;
                stack.y += stack.get_fixed(stack.stack_ix + 1)?;
                2
            }
            PointMode::XDy => {
                stack.y += stack.get_fixed(stack.stack_ix)?;
                1
            }
            PointMode::DxY => {
                stack.x += stack.get_fixed(stack.stack_ix)?;
                1
            }
            PointMode::DxMaybeDy(do_dy) => {
                stack.x += stack.get_fixed(stack.stack_ix)?;
                if do_dy {
                    stack.y += stack.get_fixed(stack.stack_ix + 1)?;
                    2
                } else {
                    1
                }
            }
            PointMode::MaybeDxDy(do_dx) => {
                stack.y += stack.get_fixed(stack.stack_ix)?;
                if do_dx {
                    stack.x += stack.get_fixed(stack.stack_ix + 1)?;
                    2
                } else {
                    1
                }
            }
        };
        stack.stack_ix += used;

        if i < N - 1 {
            // First N-1 points are control points for the cubic.
            control_points[i] = Point::new(stack.x, stack.y);
        } else {
            // Final point: emit the curve.
            path.curve_to(
                control_points[0].x,
                control_points[0].y,
                control_points[1].x,
                control_points[1].y,
                stack.x,
                stack.y,
            );
        }
    }
    Ok(())
}

/// Error variants that may occur while evaluating a Type 2 CharString operator.
#[derive(Debug, Error)]
pub enum CharStringOpError {
    /// Underlying compact font format parsing or stack error.
    #[error("CFF error: {0}")]
    Cff(#[from] CompactFontFormatError),
    /// Attempt to read more operands than available (stack underflow) while executing an operator.
    #[error("stack underflow while executing operator")]
    StackUnderflow,
    /// Attempt to execute an operator that is not yet implemented.
    #[error("unimplemented charstring operator: {0}")]
    Unimplemented(&'static str),
}

pub trait CharStringOperatorTrait {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError>;
}

trait CharStringOperator {
    const TWO_BYTE_OP_MASK: u16 = (12 << 8);
    const OPCODE: u16;

    fn new() -> Result<Self, CompactFontFormatError>
    where
        Self: Sized;
}

/// Per-operator structs expose constants for opcode and operand counts.
/// Horizontal stem hints: consumes pairs of (y, dy). Variadic by pairs.
#[allow(non_camel_case_types)]
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct HStemOp;

impl CharStringOperator for HStemOp {
    const OPCODE: u16 = 1;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for HStemOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("hstem"))
    }
}

/// Vertical stem hints: consumes pairs of (x, dx). Variadic by pairs.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct VStemOp;

impl CharStringOperator for VStemOp {
    const OPCODE: u16 = 3;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for VStemOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("vstem"))
    }
}

/// Move current point vertically by dy. First op may include width.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct VMoveToOp;

impl CharStringOperator for VMoveToOp {
    const OPCODE: u16 = 4;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(VMoveToOp)
    }
}

impl CharStringOperatorTrait for VMoveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        let mut i = 0;
        if stack.len() == 2 && !stack.have_read_width {
            stack.have_read_width = true;
            i = 1;
        }
        if !stack.is_open {
            stack.is_open = true;
        } else {
            path.close();
        }
        let delta = stack
            .get_fixed(i)
            .map_err(|_| CharStringOpError::StackUnderflow)?;
        stack.y += delta;
        path.move_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

/// Draw one or more relative lines: pairs of (dx, dy) per segment.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RLineToOp;

impl CharStringOperator for RLineToOp {
    const OPCODE: u16 = 5;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(RLineToOp)
    }
}

impl CharStringOperatorTrait for RLineToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        let mut i = 0;
        while i < stack.len() {
            let [dx, dy] = stack
                .fixed_array::<2>(i)
                .map_err(|_| CharStringOpError::StackUnderflow)?;
            stack.x += dx;
            stack.y += dy;
            path.line_to(stack.x, stack.y);
            i += 2;
        }
        stack.clear();
        Ok(())
    }
}

/// Draw alternating horizontal/vertical segments starting with horizontal.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct HLineToOp;

impl CharStringOperator for HLineToOp {
    const OPCODE: u16 = 6;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(HLineToOp)
    }
}

impl CharStringOperatorTrait for HLineToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        let mut is_x = true;
        for i in 0..stack.len() {
            let delta = stack
                .get_fixed(i)
                .map_err(|_| CharStringOpError::StackUnderflow)?;
            if is_x {
                stack.x += delta;
            } else {
                stack.y += delta;
            }
            is_x = !is_x;
            path.line_to(stack.x, stack.y);
        }
        stack.clear();
        Ok(())
    }
}

/// Draw alternating vertical/horizontal segments starting with vertical.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct VLineToOp;

impl CharStringOperator for VLineToOp {
    const OPCODE: u16 = 7;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(VLineToOp)
    }
}

impl CharStringOperatorTrait for VLineToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        let mut is_x = false;
        for i in 0..stack.len() {
            let delta = stack
                .get_fixed(i)
                .map_err(|_| CharStringOpError::StackUnderflow)?;
            if is_x {
                stack.x += delta;
            } else {
                stack.y += delta;
            }
            is_x = !is_x;
            path.line_to(stack.x, stack.y);
        }
        stack.clear();
        Ok(())
    }
}

/// Draw one or more cubic Bézier curves with relative control points: groups of 6.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RRCurveToOp;

impl CharStringOperator for RRCurveToOp {
    const OPCODE: u16 = 8;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(RRCurveToOp)
    }
}

impl CharStringOperatorTrait for RRCurveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        while stack.coords_remaining() >= 6 {
            emit_curves(stack, path, [PointMode::DxDy; 3])?;
        }
        stack.clear();
        Ok(())
    }
}

/// Call local subroutine: pops subroutine index.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct CallSubroutineOp;

impl CharStringOperator for CallSubroutineOp {
    const OPCODE: u16 = 10;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(CallSubroutineOp)
    }
}

impl CharStringOperatorTrait for CallSubroutineOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("callsubr"))
    }
}

/// Return from subroutine.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct ReturnOp;
impl CharStringOperator for ReturnOp {
    const OPCODE: u16 = 11;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for ReturnOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("return"))
    }
}

/// End the charstring.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct EndCharOp;
impl CharStringOperator for EndCharOp {
    const OPCODE: u16 = 14;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for EndCharOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        path.close();
        stack.operands.clear();
        Ok(())
    }
}

/// Horizontal stem hints (like hstem) that may be followed by hintmask.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct HStemHmOp;

impl CharStringOperator for HStemHmOp {
    const OPCODE: u16 = 18;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for HStemHmOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("hstemhm"))
    }
}

/// Hint mask: consumes pending stem hint operands, then reads mask bytes from stream.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct HintMaskOp;

impl CharStringOperator for HintMaskOp {
    const OPCODE: u16 = 19;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(HintMaskOp)
    }
}

impl CharStringOperatorTrait for HintMaskOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("hintmask"))
    }
}

/// Counter mask: like hintmask but for counter-controlled hints.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct CntrMaskOp;

impl CharStringOperator for CntrMaskOp {
    const OPCODE: u16 = 20;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(CntrMaskOp)
    }
}

impl CharStringOperatorTrait for CntrMaskOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("cntrmask"))
    }
}

/// Move current point by (dx, dy). First op may include width.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RMoveToOp;

impl CharStringOperator for RMoveToOp {
    const OPCODE: u16 = 21;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(RMoveToOp)
    }
}

impl CharStringOperatorTrait for RMoveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        let mut i = 0;
        if stack.len() == 3 && !stack.have_read_width {
            stack.have_read_width = true;
            i = 1;
        }
        if !stack.is_open {
            stack.is_open = true;
        } else {
            path.close();
        }
        let [dx, dy] = stack
            .fixed_array::<2>(i)
            .map_err(|_| CharStringOpError::StackUnderflow)?;
        stack.x += dx;
        stack.y += dy;
        path.move_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

/// Move current point horizontally by dx. First op may include width.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct HMoveToOp;

impl CharStringOperator for HMoveToOp {
    const OPCODE: u16 = 22;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(HMoveToOp)
    }
}

impl CharStringOperatorTrait for HMoveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        let mut i = 0;
        if stack.len() == 2 && !stack.have_read_width {
            stack.have_read_width = true;
            i = 1;
        }
        if !stack.is_open {
            stack.is_open = true;
        } else {
            path.close();
        }
        let delta = stack
            .get_fixed(i)
            .map_err(|_| CharStringOpError::StackUnderflow)?;
        stack.x += delta;
        path.move_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

/// Vertical stem hints (like vstem) that may be followed by hintmask.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct VStemHmOp;

impl CharStringOperator for VStemHmOp {
    const OPCODE: u16 = 23;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(VStemHmOp)
    }
}

impl CharStringOperatorTrait for VStemHmOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("vstemhm"))
    }
}

/// One or more curves followed by a final line segment: 6n + 2 operands.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RCurveLineOp;

impl CharStringOperator for RCurveLineOp {
    const OPCODE: u16 = 24;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for RCurveLineOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        while stack.coords_remaining() >= 6 {
            emit_curves(stack, path, [PointMode::DxDy; 3])?;
        }
        let [dx, dy] = stack
            .fixed_array::<2>(stack.stack_ix)
            .map_err(|_| CharStringOpError::StackUnderflow)?;
        stack.x += dx;
        stack.y += dy;
        path.line_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

/// One or more line segments followed by a final curve: 2n + 6 operands.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RLineCurveOp;

impl CharStringOperator for RLineCurveOp {
    const OPCODE: u16 = 25;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(RLineCurveOp)
    }
}

impl CharStringOperatorTrait for RLineCurveOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("rlinecurve"))
    }
}

/// Vertical-vertical curve segments. Variadic; primarily vertical tangents.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct VVCurveToOp;

impl CharStringOperator for VVCurveToOp {
    const OPCODE: u16 = 26;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(VVCurveToOp)
    }
}

impl CharStringOperatorTrait for VVCurveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        if stack.len_is_odd() {
            let dx = stack
                .get_fixed(0)
                .map_err(|_| CharStringOpError::StackUnderflow)?;
            stack.x += dx;
            stack.stack_ix = 1;
        }
        while stack.coords_remaining() > 0 {
            emit_curves(
                stack,
                path,
                [PointMode::XDy, PointMode::DxDy, PointMode::XDy],
            )?;
        }
        stack.clear();
        Ok(())
    }
}

/// Horizontal-horizontal curve segments. Variadic; primarily horizontal tangents.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct HHCurveToOp;

impl CharStringOperator for HHCurveToOp {
    const OPCODE: u16 = 27;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(HHCurveToOp)
    }
}

impl CharStringOperatorTrait for HHCurveToOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("hhcurveto"))
    }
}

/// Call global subroutine: pops global subroutine index.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct CallGSubrOp;

impl CharStringOperator for CallGSubrOp {
    const OPCODE: u16 = 29;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(CallGSubrOp)
    }
}

impl CharStringOperatorTrait for CallGSubrOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("callgsubr"))
    }
}

/// Vertical then horizontal curve segments. Variadic.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct VHCurveToOp;

impl CharStringOperator for VHCurveToOp {
    const OPCODE: u16 = 30;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(VHCurveToOp)
    }
}

impl CharStringOperatorTrait for VHCurveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        let count1 = stack.len();
        let count = count1 & !2;
        let mut is_horizontal = false;
        stack.stack_ix = count1 - count;
        while stack.stack_ix < count {
            let do_last_delta = count - stack.stack_ix == 5;
            if is_horizontal {
                emit_curves(
                    stack,
                    path,
                    [
                        PointMode::DxY,
                        PointMode::DxDy,
                        PointMode::MaybeDxDy(do_last_delta),
                    ],
                )?;
            } else {
                emit_curves(
                    stack,
                    path,
                    [
                        PointMode::XDy,
                        PointMode::DxDy,
                        PointMode::DxMaybeDy(do_last_delta),
                    ],
                )?;
            }
            is_horizontal = !is_horizontal;
        }
        stack.clear();
        Ok(())
    }
}

/// Horizontal then vertical curve segments. Variadic.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct HVCurveToOp;

impl CharStringOperator for HVCurveToOp {
    const OPCODE: u16 = 31;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for HVCurveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        let count1 = stack.len();
        let count = count1 & !2;
        let mut is_horizontal = true;
        stack.stack_ix = count1 - count;
        while stack.stack_ix < count {
            let do_last_delta = count - stack.stack_ix == 5;
            if is_horizontal {
                emit_curves(
                    stack,
                    path,
                    [
                        PointMode::DxY,
                        PointMode::DxDy,
                        PointMode::MaybeDxDy(do_last_delta),
                    ],
                )?;
            } else {
                emit_curves(
                    stack,
                    path,
                    [
                        PointMode::XDy,
                        PointMode::DxDy,
                        PointMode::DxMaybeDy(do_last_delta),
                    ],
                )?;
            }
            is_horizontal = !is_horizontal;
        }
        stack.clear();
        Ok(())
    }
}

/// Deprecated dotsection operator (ignored in Type 2). No operands.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct DotSectionOp;

impl CharStringOperator for DotSectionOp {
    const OPCODE: u16 = 12 << 8;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for DotSectionOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("dotsection"))
    }
}

/// Logical AND: pops two integers, pushes result.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct AndOp;

impl CharStringOperator for AndOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 3;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(AndOp)
    }
}

impl CharStringOperatorTrait for AndOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("and"))
    }
}

/// Logical OR: pops two integers, pushes result.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct OrOp;

impl CharStringOperator for OrOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 4;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(OrOp)
    }
}

impl CharStringOperatorTrait for OrOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("or"))
    }
}

/// Logical NOT: pops one integer, pushes result.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct NotOp;

impl CharStringOperator for NotOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 5;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(NotOp)
    }
}

impl CharStringOperatorTrait for NotOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("not"))
    }
}

/// Absolute value: pops one number, pushes abs(value).
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct AbsOp;

impl CharStringOperator for AbsOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 9;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(AbsOp)
    }
}

impl CharStringOperatorTrait for AbsOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("abs"))
    }
}

/// Addition: pops two numbers, pushes sum.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct AddOp;

impl CharStringOperator for AddOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 10;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(AddOp)
    }
}

impl CharStringOperatorTrait for AddOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("add"))
    }
}

/// Subtraction: pops two numbers, pushes difference.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct SubOp;

impl CharStringOperator for SubOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 11;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(SubOp)
    }
}

impl CharStringOperatorTrait for SubOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("sub"))
    }
}

/// Division: pops two numbers, pushes quotient.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct DivOp;

impl CharStringOperator for DivOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 12;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(DivOp)
    }
}

impl CharStringOperatorTrait for DivOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("div"))
    }
}

/// Negation: pops one number, pushes -value.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct NegOp;

impl CharStringOperator for NegOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 14;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(NegOp)
    }
}

impl CharStringOperatorTrait for NegOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("neg"))
    }
}

/// Equality test: pops two numbers, pushes 1 if equal, else 0.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct EqOp;

impl CharStringOperator for EqOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 15;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(EqOp)
    }
}

impl CharStringOperatorTrait for EqOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("eq"))
    }
}

/// Drop: pops one element and discards it.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct DropOp;

impl CharStringOperator for DropOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 18;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(DropOp)
    }
}

impl CharStringOperatorTrait for DropOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("drop"))
    }
}

/// Put: pops (index, value) and stores value in transient array at index.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct PutOp;

impl CharStringOperator for PutOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 20;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for PutOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("put"))
    }
}

/// Get: pops (index) and pushes value from transient array.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct GetOp;

impl CharStringOperator for GetOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 21;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(GetOp)
    }
}

impl CharStringOperatorTrait for GetOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("get"))
    }
}

/// If-else: pops (v1, v2, s1, s2) and pushes s1 if v1 <= v2, else s2.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct IfElseOp;

impl CharStringOperator for IfElseOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 22;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for IfElseOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("ifelse"))
    }
}

/// Random: pushes a pseudorandom number on stack; consumes none.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RandomOp;

impl CharStringOperator for RandomOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 23;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for RandomOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("random"))
    }
}

/// Multiplication: pops two numbers, pushes product.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct MulOp;

impl CharStringOperator for MulOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 24;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for MulOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("mul"))
    }
}

/// Square root: pops one number, pushes sqrt(value).
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct SqrtOp;

impl CharStringOperator for SqrtOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 26;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(SqrtOp)
    }
}

impl CharStringOperatorTrait for SqrtOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("sqrt"))
    }
}

/// Dup: duplicates the top stack element; requires at least one element.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct DupOp;

impl CharStringOperator for DupOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 27;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(DupOp)
    }
}

impl CharStringOperatorTrait for DupOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("dup"))
    }
}

/// Exch: exchanges the top two stack elements.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct ExchOp;

impl CharStringOperator for ExchOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 28;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(ExchOp)
    }
}

impl CharStringOperatorTrait for ExchOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("exch"))
    }
}

/// Index: pops (n) and duplicates the nth element (0 = top).
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct IndexOp;

impl CharStringOperator for IndexOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 29;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for IndexOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("index"))
    }
}

/// Roll: pops (n, j) and rolls the top n elements by j positions.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RollOp;

impl CharStringOperator for RollOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 30;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(RollOp)
    }
}

impl CharStringOperatorTrait for RollOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("roll"))
    }
}

/// hflex: draws a flexible curve with mostly horizontal tangents. Pops 7 numbers.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct HFlexOp;

impl CharStringOperator for HFlexOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 34;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for HFlexOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("hflex"))
    }
}

/// flex: draws a flexible curve. Pops 13 numbers.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct FlexOp;

impl CharStringOperator for FlexOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 35;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(FlexOp)
    }
}

impl CharStringOperatorTrait for FlexOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("flex"))
    }
}

/// hflex1: flexible curve variant. Pops 9 numbers.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct HFlex1Op;

impl CharStringOperator for HFlex1Op {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 36;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(HFlex1Op)
    }
}

impl CharStringOperatorTrait for HFlex1Op {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("hflex1"))
    }
}

/// flex1: flexible curve variant. Pops 11 numbers.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Flex1Op;

impl CharStringOperator for Flex1Op {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 37;

    fn new() -> Result<Self, CompactFontFormatError> {
        Ok(Flex1Op)
    }
}

impl CharStringOperatorTrait for Flex1Op {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        Err(CharStringOpError::Unimplemented("flex1"))
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct NumberOp {
    value: i32,
}

impl CharStringOperatorTrait for NumberOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringOpError> {
        stack.operands.push(self.value);
        Ok(())
    }
}

pub fn char_strings_from(
    data: &[u8],
) -> Result<Vec<Box<dyn CharStringOperatorTrait>>, CompactFontFormatError> {
    let mut ops = Vec::new();
    let mut cur = Cursor::new(data);

    while !cur.is_empty() {
        let b0 = cur.read_u8()?;
        let b_u16 = u16::from(b0);
        let op: Box<dyn CharStringOperatorTrait> = match b_u16 {
            0 | 2 | 9 | 13 | 15 | 16 | 17 => {
                // Reserved.
                return Err(CompactFontFormatError::UnexpectedDictByte(b0));
            }
            28 | 32..=254 => {
                let v = parse_int(&mut cur, b0)?;
                Box::new(NumberOp { value: v })
            }

            HStemOp::OPCODE => Box::new(HStemOp::new()?),
            VStemOp::OPCODE => Box::new(VStemOp::new()?),
            VMoveToOp::OPCODE => Box::new(VMoveToOp::new()?),
            RLineToOp::OPCODE => Box::new(RLineToOp::new()?),
            HLineToOp::OPCODE => Box::new(HLineToOp::new()?),
            VLineToOp::OPCODE => Box::new(VLineToOp::new()?),
            RRCurveToOp::OPCODE => Box::new(RRCurveToOp::new()?),
            CallSubroutineOp::OPCODE => Box::new(CallSubroutineOp::new()?),
            ReturnOp::OPCODE => Box::new(ReturnOp::new()?),
            EndCharOp::OPCODE => Box::new(EndCharOp::new()?),
            HStemHmOp::OPCODE => Box::new(HStemHmOp::new()?),
            HintMaskOp::OPCODE => Box::new(HintMaskOp::new()?),
            CntrMaskOp::OPCODE => Box::new(CntrMaskOp::new()?),
            RMoveToOp::OPCODE => Box::new(RMoveToOp::new()?),
            HMoveToOp::OPCODE => Box::new(HMoveToOp::new()?),
            VStemHmOp::OPCODE => Box::new(VStemHmOp::new()?),
            RCurveLineOp::OPCODE => Box::new(RCurveLineOp::new()?),
            RLineCurveOp::OPCODE => Box::new(RLineCurveOp::new()?),
            VVCurveToOp::OPCODE => Box::new(VVCurveToOp::new()?),
            HHCurveToOp::OPCODE => Box::new(HHCurveToOp::new()?),
            CallGSubrOp::OPCODE => Box::new(CallGSubrOp::new()?),
            VHCurveToOp::OPCODE => Box::new(VHCurveToOp::new()?),
            HVCurveToOp::OPCODE => Box::new(HVCurveToOp::new()?),
            12 => {
                let b2 = cur.read_u8()?;
                let b2_u16 = u16::from(b2) << 8;
                match b2_u16 {
                    DotSectionOp::OPCODE => Box::new(DotSectionOp::new()?),
                    AndOp::OPCODE => Box::new(AndOp::new()?),
                    OrOp::OPCODE => Box::new(OrOp::new()?),
                    NotOp::OPCODE => Box::new(NotOp::new()?),
                    AbsOp::OPCODE => Box::new(AbsOp::new()?),
                    AddOp::OPCODE => Box::new(AddOp::new()?),
                    SubOp::OPCODE => Box::new(SubOp::new()?),
                    DivOp::OPCODE => Box::new(DivOp::new()?),
                    NegOp::OPCODE => Box::new(NegOp::new()?),
                    EqOp::OPCODE => Box::new(EqOp::new()?),
                    DropOp::OPCODE => Box::new(DropOp::new()?),
                    PutOp::OPCODE => Box::new(PutOp::new()?),
                    GetOp::OPCODE => Box::new(GetOp::new()?),
                    IfElseOp::OPCODE => Box::new(IfElseOp::new()?),
                    RandomOp::OPCODE => Box::new(RandomOp::new()?),
                    MulOp::OPCODE => Box::new(MulOp::new()?),
                    SqrtOp::OPCODE => Box::new(SqrtOp::new()?),
                    DupOp::OPCODE => Box::new(DupOp::new()?),
                    ExchOp::OPCODE => Box::new(ExchOp::new()?),
                    IndexOp::OPCODE => Box::new(IndexOp::new()?),
                    RollOp::OPCODE => Box::new(RollOp::new()?),
                    HFlexOp::OPCODE => Box::new(HFlexOp::new()?),
                    FlexOp::OPCODE => Box::new(FlexOp::new()?),
                    HFlex1Op::OPCODE => Box::new(HFlex1Op::new()?),
                    Flex1Op::OPCODE => Box::new(Flex1Op::new()?),
                    _ => return Err(CompactFontFormatError::UnexpectedDictByte(0)),
                }
            }

            255 => {
                // 32-bit signed number in 16.16 fixed-point format (Type 2 CharString).
                // Read the 255 marker byte plus 4 payload bytes (big-endian two's complement).
                // We currently store operands as i32, so we truncate the fractional part.
                let b1 = cur.read_u8()?;
                let b2 = cur.read_u8()?;
                let b3 = cur.read_u8()?;
                let b4 = cur.read_u8()?;
                let raw = i32::from_be_bytes([b1, b2, b3, b4]);
                // Truncate 16.16 fixed to integer.
                let v = raw >> 16;
                Box::new(NumberOp { value: v })
            }
            _ => {
                return Err(CompactFontFormatError::UnexpectedDictByte(b0));
            }
        };
        ops.push(op);
    }
    Ok(ops)
}
