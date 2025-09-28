use crate::cff::{
    char_string_interpreter_stack::CharStringStack,
    char_string_operator::{
        AbsOp, AddOp, AndOp, CallGSubrOp, CallSubroutineOp, CntrMaskOp, DivOp, DotSectionOp,
        DropOp, DupOp, EndCharOp, EqOp, ExchOp, Flex1Op, FlexOp, GetOp, HFlex1Op, HFlexOp,
        HHCurveToOp, HLineToOp, HMoveToOp, HStemHmOp, HStemOp, HVCurveToOp, HintMaskOp, IfElseOp,
        IndexOp, MulOp, NegOp, NotOp, OrOp, PutOp, RCurveLineOp, RLineCurveOp, RLineToOp,
        RMoveToOp, RRCurveToOp, RandomOp, ReturnOp, RollOp, SqrtOp, SubOp, VHCurveToOp, VLineToOp,
        VMoveToOp, VStemHmOp, VStemOp, VVCurveToOp,
    },
};
use pdf_graphics::{pdf_path::PdfPath, point::Point};
use thiserror::Error;

/// Error variants that may occur while evaluating a CharString operator.
#[derive(Debug, Error)]
pub enum CharStringEvalError {
    #[error("arithmetic overflow while computing indices")]
    ArithmeticOverflow,
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

pub enum CharStringOperator {
    Function(fn(&mut PdfPath, &mut CharStringStack) -> Result<(), CharStringEvalError>),
    Number(i32),
}

pub trait CharStringOperatorTrait {
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError>;
}

impl CharStringOperatorTrait for RMoveToOp {
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
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
        let [dx, dy] = stack.fixed_array::<2>(i)?;
        stack.x += dx;
        stack.y += dy;
        path.move_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for HMoveToOp {
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
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
        let delta = stack.get_fixed(i)?;
        stack.x += delta;
        path.move_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for RCurveLineOp {
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        while stack.coords_remaining() >= 6 {
            emit_curves(stack, path, [PointMode::DxDy; 3])?;
        }
        let [dx, dy] = stack.fixed_array::<2>(stack.stack_index)?;
        stack.x += dx;
        stack.y += dy;
        path.line_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for HStemOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hstem"))
    }
}

impl CharStringOperatorTrait for VStemHmOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("vstemhm"))
    }
}

impl CharStringOperatorTrait for VStemOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("vstem"))
    }
}

impl CharStringOperatorTrait for VMoveToOp {
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
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
        let delta = stack.get_fixed(i)?;
        stack.y += delta;
        path.move_to(stack.x, stack.y);
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for VVCurveToOp {
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        if stack.len_is_odd() {
            let dx = stack.get_fixed(0)?;
            stack.x += dx;
            stack.stack_index = 1;
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
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        let mut i: usize = 0;
        let stack_len = stack.len();
        while i < stack_len {
            let [dx, dy] = stack.fixed_array::<2>(i)?;
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
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        let mut is_x = true;
        for i in 0..stack.len() {
            let delta = stack.get_fixed(i)?;
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
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        let mut is_x = false;
        for i in 0..stack.len() {
            let delta = stack.get_fixed(i)?;
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
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        let count1 = stack.len();
        let count = count1 & !2;
        let mut is_horizontal = false;
        stack.stack_index = count1.saturating_sub(count);
        while stack.stack_index < count {
            let do_last_delta = count.saturating_sub(stack.stack_index) == 5;
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
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        let count1 = stack.len();
        let count = count1 & !2;
        let mut is_horizontal = true;
        stack.stack_index = count1.saturating_sub(count);
        while stack.stack_index < count {
            let do_last_delta = count.saturating_sub(stack.stack_index) == 5;
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
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        while stack.coords_remaining() >= 6 {
            emit_curves(stack, path, [PointMode::DxDy; 3])?;
        }
        stack.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for CallSubroutineOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("callsubr"))
    }
}

impl CharStringOperatorTrait for ReturnOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("return"))
    }
}

impl CharStringOperatorTrait for EndCharOp {
    fn call(path: &mut PdfPath, stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        path.close();
        stack.operands.clear();
        Ok(())
    }
}

impl CharStringOperatorTrait for HStemHmOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hstemhm"))
    }
}

impl CharStringOperatorTrait for HintMaskOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hintmask"))
    }
}

impl CharStringOperatorTrait for CntrMaskOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("cntrmask"))
    }
}

impl CharStringOperatorTrait for RLineCurveOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("rlinecurve"))
    }
}

impl CharStringOperatorTrait for HHCurveToOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hhcurveto"))
    }
}

impl CharStringOperatorTrait for CallGSubrOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("callgsubr"))
    }
}

impl CharStringOperatorTrait for AndOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("and"))
    }
}

impl CharStringOperatorTrait for OrOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("or"))
    }
}

impl CharStringOperatorTrait for NotOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("not"))
    }
}

impl CharStringOperatorTrait for AbsOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("abs"))
    }
}

impl CharStringOperatorTrait for AddOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("add"))
    }
}

impl CharStringOperatorTrait for SubOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("sub"))
    }
}

impl CharStringOperatorTrait for DivOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("div"))
    }
}

impl CharStringOperatorTrait for NegOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("neg"))
    }
}

impl CharStringOperatorTrait for EqOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("eq"))
    }
}

impl CharStringOperatorTrait for DropOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("drop"))
    }
}

impl CharStringOperatorTrait for PutOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("put"))
    }
}

impl CharStringOperatorTrait for GetOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("get"))
    }
}

impl CharStringOperatorTrait for IfElseOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("ifelse"))
    }
}

impl CharStringOperatorTrait for RandomOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("random"))
    }
}

impl CharStringOperatorTrait for MulOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("mul"))
    }
}

impl CharStringOperatorTrait for SqrtOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("sqrt"))
    }
}

impl CharStringOperatorTrait for DupOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("dup"))
    }
}

impl CharStringOperatorTrait for ExchOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("exch"))
    }
}

impl CharStringOperatorTrait for IndexOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("index"))
    }
}

impl CharStringOperatorTrait for RollOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("roll"))
    }
}

impl CharStringOperatorTrait for HFlexOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hflex"))
    }
}

impl CharStringOperatorTrait for FlexOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("flex"))
    }
}

impl CharStringOperatorTrait for HFlex1Op {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("hflex1"))
    }
}

impl CharStringOperatorTrait for Flex1Op {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
        Err(CharStringEvalError::Unimplemented("flex1"))
    }
}

impl CharStringOperatorTrait for DotSectionOp {
    fn call(_path: &mut PdfPath, _stack: &mut CharStringStack) -> Result<(), CharStringEvalError> {
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
                let ix0 = stack.stack_index;
                let ix1 = checked_add_usize(ix0, 1)?;
                stack.x += stack.get_fixed(ix0)?;
                stack.y += stack.get_fixed(ix1)?;
                2
            }
            PointMode::XDy => {
                let ix0 = stack.stack_index;
                stack.y += stack.get_fixed(ix0)?;
                1
            }
            PointMode::DxY => {
                let ix0 = stack.stack_index;
                stack.x += stack.get_fixed(ix0)?;
                1
            }
            PointMode::DxMaybeDy(do_dy) => {
                let ix0 = stack.stack_index;
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
                let ix0 = stack.stack_index;
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
        stack.stack_index = checked_add_usize(stack.stack_index, used)?;

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
