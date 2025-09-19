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

use pdf_graphics::pdf_path::PdfPath;

use crate::cff::{cursor::Cursor, error::CompactFontFormatError};

pub trait CharStringOperatorTrait {
    fn call(&self, path: &mut PdfPath);
}

#[inline]
#[allow(clippy::as_conversions)]
fn f32_from_i32(v: i32) -> f32 {
    v as f32
}

trait CharStringOperator {
    const TWO_BYTE_OP_MASK: u16 = (12 << 8);
    const OPCODE: u16;
    const MIN_OPERANDS: usize;
    const MAX_OPERANDS: usize;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError>
    where
        Self: Sized;
}

/// Per-operator structs expose constants for opcode and operand counts.
/// Horizontal stem hints: consumes pairs of (y, dy). Variadic by pairs.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HStemOp {
    operands: Vec<i32>,
}

impl CharStringOperator for HStemOp {
    const OPCODE: u16 = 1;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for HStemOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Vertical stem hints: consumes pairs of (x, dx). Variadic by pairs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VStemOp {
    operands: Vec<i32>,
}

impl CharStringOperator for VStemOp {
    const OPCODE: u16 = 3;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for VStemOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Move current point vertically by dy. First op may include width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VMoveToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for VMoveToOp {
    const OPCODE: u16 = 4;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(VMoveToOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for VMoveToOp {
    fn call(&self, path: &mut PdfPath) {
        println!("{:?}", self);
        let dy = if self.operands.len() == 2 {
            self.operands[1]
        } else {
            self.operands[0]
        };
        path.move_rel(0.0, f32_from_i32(dy));
    }
}

/// Draw one or more relative lines: pairs of (dx, dy) per segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RLineToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for RLineToOp {
    const OPCODE: u16 = 5;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(RLineToOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for RLineToOp {
    fn call(&self, path: &mut PdfPath) {
        println!("{:?}", self);
        let mut it = self.operands.chunks_exact(2);
        for pair in &mut it {
            let dx = f32_from_i32(pair[0]);
            let dy = f32_from_i32(pair[1]);
            path.line_rel(dx, dy);
        }
    }
}

/// Draw alternating horizontal/vertical segments starting with horizontal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HLineToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for HLineToOp {
    const OPCODE: u16 = 6;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(HLineToOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for HLineToOp {
    fn call(&self, path: &mut PdfPath) {
        println!("{:?}", self);
        let mut horizontal = true;
        for &d in &self.operands {
            if horizontal {
                path.line_rel(f32_from_i32(d), 0.0);
            } else {
                path.line_rel(0.0, f32_from_i32(d));
            }
            horizontal = !horizontal;
        }
    }
}

/// Draw alternating vertical/horizontal segments starting with vertical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VLineToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for VLineToOp {
    const OPCODE: u16 = 7;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(VLineToOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for VLineToOp {
    fn call(&self, path: &mut PdfPath) {
        println!("{:?}", self);
        let mut vertical = true;
        for &d in &self.operands {
            if vertical {
                path.line_rel(0.0, f32_from_i32(d));
            } else {
                path.line_rel(f32_from_i32(d), 0.0);
            }
            vertical = !vertical;
        }
    }
}

/// Draw one or more cubic Bézier curves with relative control points: groups of 6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RRCurveToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for RRCurveToOp {
    const OPCODE: u16 = 8;
    const MIN_OPERANDS: usize = 6;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(RRCurveToOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for RRCurveToOp {
    fn call(&self, path: &mut PdfPath) {
        println!("{:?}", self);
        for chunk in self.operands.chunks_exact(6) {
            path.curve_rel(
                f32_from_i32(chunk[0]),
                f32_from_i32(chunk[1]),
                f32_from_i32(chunk[2]),
                f32_from_i32(chunk[3]),
                f32_from_i32(chunk[4]),
                f32_from_i32(chunk[5]),
            );
        }
    }
}

/// Call local subroutine: pops subroutine index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSubroutineOp {
    operands: Vec<i32>,
}

impl CharStringOperator for CallSubroutineOp {
    const OPCODE: u16 = 10;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 1;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(CallSubroutineOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for CallSubroutineOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Return from subroutine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnOp;
impl CharStringOperator for ReturnOp {
    const OPCODE: u16 = 11;
    const MIN_OPERANDS: usize = 0;
    const MAX_OPERANDS: usize = 0;

    fn from(_operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for ReturnOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// End the charstring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndCharOp;
impl CharStringOperator for EndCharOp {
    const OPCODE: u16 = 14;
    const MIN_OPERANDS: usize = 0;
    const MAX_OPERANDS: usize = 0;

    fn from(_operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for EndCharOp {
    fn call(&self, path: &mut PdfPath) {
        println!("{:?}", self);
        path.close();
    }
}

/// Horizontal stem hints (like hstem) that may be followed by hintmask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HStemHmOp {
    operands: Vec<i32>,
}

impl CharStringOperator for HStemHmOp {
    const OPCODE: u16 = 18;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for HStemHmOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Hint mask: consumes pending stem hint operands, then reads mask bytes from stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintMaskOp {
    operands: Vec<i32>,
}

impl CharStringOperator for HintMaskOp {
    const OPCODE: u16 = 19;
    const MIN_OPERANDS: usize = 0;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(HintMaskOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for HintMaskOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Counter mask: like hintmask but for counter-controlled hints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CntrMaskOp {
    operands: Vec<i32>,
}

impl CharStringOperator for CntrMaskOp {
    const OPCODE: u16 = 20;
    const MIN_OPERANDS: usize = 0;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(CntrMaskOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for CntrMaskOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Move current point by (dx, dy). First op may include width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RMoveToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for RMoveToOp {
    const OPCODE: u16 = 21;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 3;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(RMoveToOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for RMoveToOp {
    fn call(&self, path: &mut PdfPath) {
        println!("{:?}", self);
        let (dx, dy) = match self.operands.as_slice() {
            // width, dx, dy
            [_, dx, dy] => (*dx, *dy),
            // dx, dy
            [dx, dy] => (*dx, *dy),
            _ => (0, 0),
        };
        path.move_rel(f32_from_i32(dx), f32_from_i32(dy));
    }
}

/// Move current point horizontally by dx. First op may include width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HMoveToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for HMoveToOp {
    const OPCODE: u16 = 22;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(HMoveToOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for HMoveToOp {
    fn call(&self, path: &mut PdfPath) {
        println!("{:?}", self);
        let dx = if self.operands.len() == 2 {
            self.operands[1]
        } else {
            self.operands[0]
        };
        path.move_rel(f32_from_i32(dx), 0.0);
    }
}

/// Vertical stem hints (like vstem) that may be followed by hintmask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VStemHmOp {
    operands: Vec<i32>,
}

impl CharStringOperator for VStemHmOp {
    const OPCODE: u16 = 23;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(VStemHmOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for VStemHmOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// One or more curves followed by a final line segment: 6n + 2 operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RCurveLineOp {
    operands: Vec<i32>,
}

impl CharStringOperator for RCurveLineOp {
    const OPCODE: u16 = 24;
    const MIN_OPERANDS: usize = 8; // one curve (6) + one line (2)
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for RCurveLineOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// One or more line segments followed by a final curve: 2n + 6 operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RLineCurveOp {
    operands: Vec<i32>,
}

impl CharStringOperator for RLineCurveOp {
    const OPCODE: u16 = 25;
    const MIN_OPERANDS: usize = 8;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(RLineCurveOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for RLineCurveOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Vertical-vertical curve segments. Variadic; primarily vertical tangents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VVCurveToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for VVCurveToOp {
    const OPCODE: u16 = 26;
    const MIN_OPERANDS: usize = 4;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        if operands.len() < Self::MIN_OPERANDS {
            return Err(CompactFontFormatError::InsufficientOperands {
                expected: Self::MIN_OPERANDS,
                found: operands.len(),
            });
        }
        Ok(VVCurveToOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for VVCurveToOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Horizontal-horizontal curve segments. Variadic; primarily horizontal tangents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HHCurveToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for HHCurveToOp {
    const OPCODE: u16 = 27;
    const MIN_OPERANDS: usize = 4;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        if operands.len() < Self::MIN_OPERANDS {
            return Err(CompactFontFormatError::InsufficientOperands {
                expected: Self::MIN_OPERANDS,
                found: operands.len(),
            });
        }
        Ok(HHCurveToOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for HHCurveToOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Operand encoding marker for a 16-bit integer (not an operator). Pops nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortIntOp {
    operands: Vec<i32>,
}

impl CharStringOperator for ShortIntOp {
    const OPCODE: u16 = 28;
    const MIN_OPERANDS: usize = 0;
    const MAX_OPERANDS: usize = 0;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(ShortIntOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for ShortIntOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Call global subroutine: pops global subroutine index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallGSubrOp {
    operands: Vec<i32>,
}

impl CharStringOperator for CallGSubrOp {
    const OPCODE: u16 = 29;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 1;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(CallGSubrOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for CallGSubrOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Vertical then horizontal curve segments. Variadic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VHCurveToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for VHCurveToOp {
    const OPCODE: u16 = 30;
    const MIN_OPERANDS: usize = 4;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(VHCurveToOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for VHCurveToOp {
    fn call(&self, path: &mut PdfPath) {
        println!("{:?}", self);
        let mut rem: &[i32] = &self.operands;
        while rem.len() >= 4 {
            if rem.len() == 5 {
                let dy1 = f32_from_i32(rem[0]);
                let dx2 = f32_from_i32(rem[1]);
                let dy2 = f32_from_i32(rem[2]);
                let dx3 = f32_from_i32(rem[3]);
                let dy3 = f32_from_i32(rem[4]);
                path.curve_rel(0.0, dy1, dx2, dy2, dx3, dy3);
                rem = &rem[5..];
            } else {
                let dy1 = f32_from_i32(rem[0]);
                let dx2 = f32_from_i32(rem[1]);
                let dy2 = f32_from_i32(rem[2]);
                let dx3 = f32_from_i32(rem[3]);
                path.curve_rel(0.0, dy1, dx2, dy2, dx3, 0.0);
                rem = &rem[4..];
            }
        }
    }
}

/// Horizontal then vertical curve segments. Variadic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HVCurveToOp {
    operands: Vec<i32>,
}

impl CharStringOperator for HVCurveToOp {
    const OPCODE: u16 = 31;
    const MIN_OPERANDS: usize = 4;
    const MAX_OPERANDS: usize = usize::MAX;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for HVCurveToOp {
    fn call(&self, path: &mut PdfPath) {
        println!("{:?}", self);
        let mut rem: &[i32] = &self.operands;
        while rem.len() >= 4 {
            if rem.len() == 5 {
                let dx1 = f32_from_i32(rem[0]);
                let dx2 = f32_from_i32(rem[1]);
                let dy2 = f32_from_i32(rem[2]);
                let dy3 = f32_from_i32(rem[3]);
                let dx3 = f32_from_i32(rem[4]);
                path.curve_rel(dx1, 0.0, dx2, dy2, dx3, dy3);
                rem = &rem[5..];
            } else {
                let dx1 = f32_from_i32(rem[0]);
                let dx2 = f32_from_i32(rem[1]);
                let dy2 = f32_from_i32(rem[2]);
                let dy3 = f32_from_i32(rem[3]);
                path.curve_rel(dx1, 0.0, dx2, dy2, 0.0, dy3);
                rem = &rem[4..];
            }
        }
    }
}

/// Deprecated dotsection operator (ignored in Type 2). No operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotSectionOp {
    operands: Vec<i32>,
}

impl CharStringOperator for DotSectionOp {
    const OPCODE: u16 = 12 << 8;
    const MIN_OPERANDS: usize = 0;
    const MAX_OPERANDS: usize = 0;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for DotSectionOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Logical AND: pops two integers, pushes result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndOp {
    operands: Vec<i32>,
}

impl CharStringOperator for AndOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 3;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(AndOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for AndOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Logical OR: pops two integers, pushes result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrOp {
    operands: Vec<i32>,
}

impl CharStringOperator for OrOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 4;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(OrOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for OrOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Logical NOT: pops one integer, pushes result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotOp {
    operands: Vec<i32>,
}

impl CharStringOperator for NotOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 5;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 1;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(NotOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for NotOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Absolute value: pops one number, pushes abs(value).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsOp {
    operands: Vec<i32>,
}

impl CharStringOperator for AbsOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 9;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 1;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(AbsOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for AbsOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Addition: pops two numbers, pushes sum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddOp {
    operands: Vec<i32>,
}

impl CharStringOperator for AddOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 10;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(AddOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for AddOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Subtraction: pops two numbers, pushes difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubOp {
    operands: Vec<i32>,
}

impl CharStringOperator for SubOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 11;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(SubOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for SubOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Division: pops two numbers, pushes quotient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivOp {
    operands: Vec<i32>,
}

impl CharStringOperator for DivOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 12;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(DivOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for DivOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Negation: pops one number, pushes -value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegOp {
    operands: Vec<i32>,
}

impl CharStringOperator for NegOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 14;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 1;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(NegOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for NegOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Equality test: pops two numbers, pushes 1 if equal, else 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqOp {
    operands: Vec<i32>,
}

impl CharStringOperator for EqOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 15;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(EqOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for EqOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Drop: pops one element and discards it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropOp {
    operands: Vec<i32>,
}

impl CharStringOperator for DropOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 18;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 1;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(DropOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for DropOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Put: pops (index, value) and stores value in transient array at index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutOp {
    operands: Vec<i32>,
}

impl CharStringOperator for PutOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 20;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for PutOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Get: pops (index) and pushes value from transient array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetOp {
    operands: Vec<i32>,
}

impl CharStringOperator for GetOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 21;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 1;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(GetOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for GetOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// If-else: pops (v1, v2, s1, s2) and pushes s1 if v1 <= v2, else s2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfElseOp {
    operands: Vec<i32>,
}

impl CharStringOperator for IfElseOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 22;
    const MIN_OPERANDS: usize = 4;
    const MAX_OPERANDS: usize = 4;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for IfElseOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Random: pushes a pseudorandom number on stack; consumes none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandomOp;

impl CharStringOperator for RandomOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 23;
    const MIN_OPERANDS: usize = 0;
    const MAX_OPERANDS: usize = 0;

    fn from(_operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self)
    }
}

impl CharStringOperatorTrait for RandomOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Multiplication: pops two numbers, pushes product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MulOp {
    operands: Vec<i32>,
}

impl CharStringOperator for MulOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 24;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for MulOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Square root: pops one number, pushes sqrt(value).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqrtOp {
    operands: Vec<i32>,
}

impl CharStringOperator for SqrtOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 26;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 1;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(SqrtOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for SqrtOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Dup: duplicates the top stack element; requires at least one element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DupOp {
    operands: Vec<i32>,
}

impl CharStringOperator for DupOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 27;
    const MIN_OPERANDS: usize = 1; // requires one available
    const MAX_OPERANDS: usize = 1; // considered as consuming one for accounting

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(DupOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for DupOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Exch: exchanges the top two stack elements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchOp {
    operands: Vec<i32>,
}

impl CharStringOperator for ExchOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 28;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(ExchOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for ExchOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Index: pops (n) and duplicates the nth element (0 = top).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexOp {
    operands: Vec<i32>,
}

impl CharStringOperator for IndexOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 29;
    const MIN_OPERANDS: usize = 1;
    const MAX_OPERANDS: usize = 1;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for IndexOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// Roll: pops (n, j) and rolls the top n elements by j positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollOp {
    operands: Vec<i32>,
}

impl CharStringOperator for RollOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 30;
    const MIN_OPERANDS: usize = 2;
    const MAX_OPERANDS: usize = 2;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(RollOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for RollOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// hflex: draws a flexible curve with mostly horizontal tangents. Pops 7 numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HFlexOp {
    operands: Vec<i32>,
}

impl CharStringOperator for HFlexOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 34;
    const MIN_OPERANDS: usize = 7;
    const MAX_OPERANDS: usize = 7;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Self {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for HFlexOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// flex: draws a flexible curve. Pops 13 numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlexOp {
    operands: Vec<i32>,
}

impl CharStringOperator for FlexOp {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 35;
    const MIN_OPERANDS: usize = 13;
    const MAX_OPERANDS: usize = 13;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(FlexOp {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for FlexOp {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// hflex1: flexible curve variant. Pops 9 numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HFlex1Op {
    operands: Vec<i32>,
}

impl CharStringOperator for HFlex1Op {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 36;
    const MIN_OPERANDS: usize = 9;
    const MAX_OPERANDS: usize = 9;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(HFlex1Op {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for HFlex1Op {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}

/// flex1: flexible curve variant. Pops 11 numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flex1Op {
    operands: Vec<i32>,
}

impl CharStringOperator for Flex1Op {
    const OPCODE: u16 = Self::TWO_BYTE_OP_MASK | 37;
    const MIN_OPERANDS: usize = 11;
    const MAX_OPERANDS: usize = 11;

    fn from(operands: &[i32]) -> Result<Self, CompactFontFormatError> {
        Ok(Flex1Op {
            operands: operands.to_vec(),
        })
    }
}

impl CharStringOperatorTrait for Flex1Op {
    fn call(&self, _path: &mut PdfPath) {
        println!("{:?}", self);
    }
}
fn construct_op<T: CharStringOperator + CharStringOperatorTrait + 'static>(
    operands: &[i32],
) -> Result<Box<dyn CharStringOperatorTrait>, CompactFontFormatError> {
    if operands.len() < T::MIN_OPERANDS {
        return Err(CompactFontFormatError::InvalidOperandCount {
            expected: T::MIN_OPERANDS.to_string(),
            found: operands.len(),
        });
    }

    if operands.len() > T::MAX_OPERANDS
        && T::MIN_OPERANDS != 0
        && operands.len().checked_rem(T::MIN_OPERANDS).unwrap_or(1) != 0
    {
        return Err(CompactFontFormatError::InvalidOperandCount {
            expected: T::MAX_OPERANDS.to_string(),
            found: operands.len(),
        });
    }

    Ok(Box::new(T::from(operands)?))
}

pub fn char_strings_from(
    data: &[u8],
) -> Result<Vec<Box<dyn CharStringOperatorTrait>>, CompactFontFormatError> {
    let mut ops = Vec::new();
    let mut cur = Cursor::new(data);
    let mut operands = Vec::new();

    while !cur.is_empty() {
        let b = cur.peek_u8()?;
        let b_u16 = u16::from(b);
        let op = match b_u16 {
            0 | 2 | 9 | 13 | 15 | 16 | 17 => {
                // Reserved.
                return Err(CompactFontFormatError::UnexpectedDictByte(b));
            }
            HStemOp::OPCODE => construct_op::<HStemOp>(&operands)?,
            VStemOp::OPCODE => construct_op::<VStemOp>(&operands)?,
            VMoveToOp::OPCODE => construct_op::<VMoveToOp>(&operands)?,
            RLineToOp::OPCODE => construct_op::<RLineToOp>(&operands)?,
            HLineToOp::OPCODE => construct_op::<HLineToOp>(&operands)?,
            VLineToOp::OPCODE => construct_op::<VLineToOp>(&operands)?,
            RRCurveToOp::OPCODE => construct_op::<RRCurveToOp>(&operands)?,
            CallSubroutineOp::OPCODE => construct_op::<CallSubroutineOp>(&operands)?,
            ReturnOp::OPCODE => construct_op::<ReturnOp>(&operands)?,
            EndCharOp::OPCODE => construct_op::<EndCharOp>(&operands)?,
            HStemHmOp::OPCODE => construct_op::<HStemHmOp>(&operands)?,
            HintMaskOp::OPCODE => construct_op::<HintMaskOp>(&operands)?,
            CntrMaskOp::OPCODE => construct_op::<CntrMaskOp>(&operands)?,
            RMoveToOp::OPCODE => construct_op::<RMoveToOp>(&operands)?,
            HMoveToOp::OPCODE => construct_op::<HMoveToOp>(&operands)?,
            VStemHmOp::OPCODE => construct_op::<VStemHmOp>(&operands)?,
            RCurveLineOp::OPCODE => construct_op::<RCurveLineOp>(&operands)?,
            RLineCurveOp::OPCODE => construct_op::<RLineCurveOp>(&operands)?,
            VVCurveToOp::OPCODE => construct_op::<VVCurveToOp>(&operands)?,
            HHCurveToOp::OPCODE => construct_op::<HHCurveToOp>(&operands)?,
            ShortIntOp::OPCODE => construct_op::<ShortIntOp>(&operands)?,
            CallGSubrOp::OPCODE => construct_op::<CallGSubrOp>(&operands)?,
            VHCurveToOp::OPCODE => construct_op::<VHCurveToOp>(&operands)?,
            HVCurveToOp::OPCODE => construct_op::<HVCurveToOp>(&operands)?,
            12 => {
                let b2 = cur.read_u16()?;
                let op = match b2 {
                    DotSectionOp::OPCODE => construct_op::<DotSectionOp>(&operands)?,
                    AndOp::OPCODE => construct_op::<AndOp>(&operands)?,
                    OrOp::OPCODE => construct_op::<OrOp>(&operands)?,
                    NotOp::OPCODE => construct_op::<NotOp>(&operands)?,
                    AbsOp::OPCODE => construct_op::<AbsOp>(&operands)?,
                    AddOp::OPCODE => construct_op::<AddOp>(&operands)?,
                    SubOp::OPCODE => construct_op::<SubOp>(&operands)?,
                    DivOp::OPCODE => construct_op::<DivOp>(&operands)?,
                    NegOp::OPCODE => construct_op::<NegOp>(&operands)?,
                    EqOp::OPCODE => construct_op::<EqOp>(&operands)?,
                    DropOp::OPCODE => construct_op::<DropOp>(&operands)?,
                    PutOp::OPCODE => construct_op::<PutOp>(&operands)?,
                    GetOp::OPCODE => construct_op::<GetOp>(&operands)?,
                    IfElseOp::OPCODE => construct_op::<IfElseOp>(&operands)?,
                    RandomOp::OPCODE => construct_op::<RandomOp>(&operands)?,
                    MulOp::OPCODE => construct_op::<MulOp>(&operands)?,
                    SqrtOp::OPCODE => construct_op::<SqrtOp>(&operands)?,
                    DupOp::OPCODE => construct_op::<DupOp>(&operands)?,
                    ExchOp::OPCODE => construct_op::<ExchOp>(&operands)?,
                    IndexOp::OPCODE => construct_op::<IndexOp>(&operands)?,
                    RollOp::OPCODE => construct_op::<RollOp>(&operands)?,
                    HFlexOp::OPCODE => construct_op::<HFlexOp>(&operands)?,
                    FlexOp::OPCODE => construct_op::<FlexOp>(&operands)?,
                    HFlex1Op::OPCODE => construct_op::<HFlex1Op>(&operands)?,
                    Flex1Op::OPCODE => construct_op::<Flex1Op>(&operands)?,
                    _ => return Err(CompactFontFormatError::UnexpectedDictByte(0)),
                };
                ops.push(op);
                operands.clear();
                continue;
            }
            32..=246 => {
                // One-byte integer: encodes values in the range [-107, 107].
                let b0 = i32::from(cur.read_u8()?);
                let v = b0
                    .checked_sub(139)
                    .ok_or(CompactFontFormatError::OperandOverflow)?;
                operands.push(v);
                continue;
            }
            247..=250 => {
                // Two-byte positive integer: encodes values in the range [108, 1131].
                // Value = (b0 - 247) * 256 + b1 + 108
                let b0 = i32::from(cur.read_u8()?);
                let b1 = i32::from(cur.read_u8()?);
                let v = b0
                    .checked_sub(247)
                    .and_then(|v| v.checked_mul(256))
                    .and_then(|v| v.checked_add(b1))
                    .and_then(|v| v.checked_add(108))
                    .ok_or(CompactFontFormatError::OperandOverflow)?;
                operands.push(v);
                continue;
            }
            251..=254 => {
                // Two-byte negative integer: encodes values in the range [-1131, -108].
                // Value = -((b0 - 251) * 256) - b1 - 108
                let b0 = i32::from(cur.read_u8()?);
                let b1 = i32::from(cur.read_u8()?);
                let v = b0
                    .checked_sub(251)
                    .and_then(|v| v.checked_mul(256))
                    .and_then(|v| v.checked_sub(b1))
                    .and_then(|v| v.checked_sub(108))
                    .and_then(i32::checked_neg)
                    .ok_or(CompactFontFormatError::OperandOverflow)?;
                operands.push(v);
                continue;
            }
            255 => {
                // Consume the op code.
                let _ = cur.read_u8()?;

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
                operands.push(v);
                continue;
            }
            _ => {
                return Err(CompactFontFormatError::UnexpectedDictByte(b));
            }
        };
        ops.push(op);
        operands.clear();
        cur.advance()?;
    }
    Ok(ops)
}
