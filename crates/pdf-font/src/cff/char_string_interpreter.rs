use crate::cff::{
    char_string_interpreter_stack::CharStringStack,
    char_string_operator::{
        AbsOp, AddOp, AndOp, CallGSubrOp, CallSubroutineOp, CntrMaskOp, DivOp, DotSectionOp,
        DropOp, DupOp, EndCharOp, EqOp, ExchOp, Flex1Op, FlexOp, GetOp, HFlex1Op, HFlexOp,
        HHCurveToOp, HLineToOp, HMoveToOp, HStemHmOp, HStemOp, HVCurveToOp, HintMaskOp, IfElseOp,
        IndexOp, MulOp, NegOp, NotOp, NumberOp, OrOp, PutOp, RCurveLineOp, RLineCurveOp, RLineToOp,
        RMoveToOp, RRCurveToOp, RandomOp, ReturnOp, RollOp, SqrtOp, SubOp, VHCurveToOp, VLineToOp,
        VMoveToOp, VStemHmOp, VStemOp, VVCurveToOp,
    },
    error::CompactFontFormatError,
};
use pdf_graphics::{pdf_path::PdfPath, point::Point};
use thiserror::Error;

/// Error variants that may occur while evaluating a Type 2 CharString operator.
#[derive(Debug, Error)]
pub enum CharStringEvalError {
    /// Underlying compact font format parsing or stack error.
    #[error("CFF error: {0}")]
    Cff(#[from] CompactFontFormatError),
    /// Attempt to read more operands than available (stack underflow) while executing an operator.
    #[error("stack underflow while executing operator")]
    StackUnderflow,
    /// Arithmetic overflow encountered while computing operand indices.
    #[error("arithmetic overflow while computing indices")]
    ArithmeticOverflow,
    /// Attempt to execute an operator that is not yet implemented.
    #[error("unimplemented charstring operator: {0}")]
    Unimplemented(&'static str),
    #[error("{0}")]
    CharStringStackError(#[from] crate::cff::char_string_interpreter_stack::CharStringStackError),
}

/// Specifies how point coordinates for a curve are computed.
#[derive(Copy, Clone)]
enum PointMode {
    DxDy,
    XDy,
    DxY,
    DxMaybeDy(bool),
    MaybeDxDy(bool),
}

pub trait CharStringOperatorTrait {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError>;
}

impl CharStringOperatorTrait for NumberOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        stack.operands.push(self.value);
        Ok(())
    }
}

impl CharStringOperatorTrait for RMoveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
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
            .map_err(|_| CharStringEvalError::StackUnderflow)?;
        stack.x += dx;
        stack.y += dy;
        path.move_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for HMoveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
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
            .map_err(|_| CharStringEvalError::StackUnderflow)?;
        stack.x += delta;
        path.move_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for RCurveLineOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        while stack.coords_remaining() >= 6 {
            emit_curves(stack, path, [PointMode::DxDy; 3])?;
        }
        let [dx, dy] = stack
            .fixed_array::<2>(stack.stack_ix)
            .map_err(|_| CharStringEvalError::StackUnderflow)?;
        stack.x += dx;
        stack.y += dy;
        path.line_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for HStemOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hstem"))
    }
}

impl CharStringOperatorTrait for VStemHmOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("vstemhm"))
    }
}

impl CharStringOperatorTrait for VStemOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("vstem"))
    }
}

impl CharStringOperatorTrait for VMoveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
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
            .map_err(|_| CharStringEvalError::StackUnderflow)?;
        stack.y += delta;
        path.move_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for VVCurveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        if stack.len_is_odd() {
            let dx = stack
                .get_fixed(0)
                .map_err(|_| CharStringEvalError::StackUnderflow)?;
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

impl CharStringOperatorTrait for RLineToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        let mut i: usize = 0;
        let stack_len = stack.len();
        while i < stack_len {
            let [dx, dy] = stack
                .fixed_array::<2>(i)
                .map_err(|_| CharStringEvalError::StackUnderflow)?;
            stack.x += dx;
            stack.y += dy;
            path.line_to(stack.x, stack.y);
            i = checked_add_usize(i, 2)?;
        }
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for HLineToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        let mut is_x = true;
        for i in 0..stack.len() {
            let delta = stack
                .get_fixed(i)
                .map_err(|_| CharStringEvalError::StackUnderflow)?;
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

impl CharStringOperatorTrait for VLineToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        let mut is_x = false;
        for i in 0..stack.len() {
            let delta = stack
                .get_fixed(i)
                .map_err(|_| CharStringEvalError::StackUnderflow)?;
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

impl CharStringOperatorTrait for VHCurveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        let count1 = stack.len();
        let count = count1 & !2;
        let mut is_horizontal = false;
        stack.stack_ix = count1.saturating_sub(count);
        while stack.stack_ix < count {
            let do_last_delta = count.saturating_sub(stack.stack_ix) == 5;
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

impl CharStringOperatorTrait for HVCurveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        let count1 = stack.len();
        let count = count1 & !2;
        let mut is_horizontal = true;
        stack.stack_ix = count1.saturating_sub(count);
        while stack.stack_ix < count {
            let do_last_delta = count.saturating_sub(stack.stack_ix) == 5;
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

impl CharStringOperatorTrait for RRCurveToOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        while stack.coords_remaining() >= 6 {
            emit_curves(stack, path, [PointMode::DxDy; 3])?;
        }
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for CallSubroutineOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("callsubr"))
    }
}

impl CharStringOperatorTrait for ReturnOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("return"))
    }
}

impl CharStringOperatorTrait for EndCharOp {
    fn call(
        &self,
        path: &mut PdfPath,
        stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        path.close();
        stack.operands.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for HStemHmOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hstemhm"))
    }
}

impl CharStringOperatorTrait for HintMaskOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hintmask"))
    }
}

impl CharStringOperatorTrait for CntrMaskOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("cntrmask"))
    }
}

impl CharStringOperatorTrait for RLineCurveOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("rlinecurve"))
    }
}

impl CharStringOperatorTrait for HHCurveToOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hhcurveto"))
    }
}

impl CharStringOperatorTrait for CallGSubrOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("callgsubr"))
    }
}

impl CharStringOperatorTrait for AndOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("and"))
    }
}

impl CharStringOperatorTrait for OrOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("or"))
    }
}

impl CharStringOperatorTrait for NotOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("not"))
    }
}

impl CharStringOperatorTrait for AbsOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("abs"))
    }
}

impl CharStringOperatorTrait for AddOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("add"))
    }
}

impl CharStringOperatorTrait for SubOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("sub"))
    }
}

impl CharStringOperatorTrait for DivOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("div"))
    }
}

impl CharStringOperatorTrait for NegOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("neg"))
    }
}

impl CharStringOperatorTrait for EqOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("eq"))
    }
}

impl CharStringOperatorTrait for DropOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("drop"))
    }
}

impl CharStringOperatorTrait for PutOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("put"))
    }
}

impl CharStringOperatorTrait for GetOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("get"))
    }
}

impl CharStringOperatorTrait for IfElseOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("ifelse"))
    }
}

impl CharStringOperatorTrait for RandomOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("random"))
    }
}

impl CharStringOperatorTrait for MulOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("mul"))
    }
}

impl CharStringOperatorTrait for SqrtOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("sqrt"))
    }
}

impl CharStringOperatorTrait for DupOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("dup"))
    }
}

impl CharStringOperatorTrait for ExchOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("exch"))
    }
}

impl CharStringOperatorTrait for IndexOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("index"))
    }
}

impl CharStringOperatorTrait for RollOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("roll"))
    }
}

impl CharStringOperatorTrait for HFlexOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hflex"))
    }
}

impl CharStringOperatorTrait for FlexOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("flex"))
    }
}

impl CharStringOperatorTrait for HFlex1Op {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hflex1"))
    }
}

impl CharStringOperatorTrait for Flex1Op {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("flex1"))
    }
}

impl CharStringOperatorTrait for DotSectionOp {
    fn call(
        &self,
        _path: &mut PdfPath,
        _stack: &mut CharStringStack,
    ) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("dotsection"))
    }
}

/// Safely add two `usize` values, returning a `CharStringOpError::ArithmeticOverflow` on overflow.
fn checked_add_usize(a: usize, b: usize) -> Result<usize, CharStringEvalError> {
    a.checked_add(b)
        .ok_or(CharStringEvalError::ArithmeticOverflow)
}

fn emit_curves(
    stack: &mut CharStringStack,
    path: &mut PdfPath,
    modes: [PointMode; 3],
) -> Result<(), CharStringEvalError> {
    let mut control_points = [Point::default(); 2];

    for (i, mode) in modes.into_iter().enumerate() {
        let used = match mode {
            PointMode::DxDy => {
                let ix0 = stack.stack_ix;
                let ix1 = checked_add_usize(ix0, 1)?;
                stack.x += stack.get_fixed(ix0)?;
                stack.y += stack.get_fixed(ix1)?;
                2
            }
            PointMode::XDy => {
                let ix0 = stack.stack_ix;
                stack.y += stack.get_fixed(ix0)?;
                1
            }
            PointMode::DxY => {
                let ix0 = stack.stack_ix;
                stack.x += stack.get_fixed(ix0)?;
                1
            }
            PointMode::DxMaybeDy(do_dy) => {
                let ix0 = stack.stack_ix;
                stack.x += stack.get_fixed(ix0)?;
                if do_dy {
                    let ix1 = checked_add_usize(ix0, 1)?;
                    stack.y += stack.get_fixed(ix1)?;
                    2
                } else {
                    1
                }
            }
            PointMode::MaybeDxDy(do_dx) => {
                let ix0 = stack.stack_ix;
                stack.y += stack.get_fixed(ix0)?;
                if do_dx {
                    let ix1 = checked_add_usize(ix0, 1)?;
                    stack.x += stack.get_fixed(ix1)?;
                    2
                } else {
                    1
                }
            }
        };
        stack.stack_ix = checked_add_usize(stack.stack_ix, used)?;

        let is_not_last = i.checked_add(1).map(|v| v < modes.len()).unwrap_or(false);
        if is_not_last {
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
