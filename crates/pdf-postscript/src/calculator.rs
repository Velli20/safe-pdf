use crate::{operator::Operator, parser::parse, value::Value};
use num_traits::ToPrimitive;
use thiserror::Error;

/// Errors that can occur while executing a PostScript-like calculator program.
#[derive(Debug, Error, PartialEq)]
pub enum CalcError {
    #[error("program is not enclosed in a '{{ }}' procedure")]
    MissingProgramProcedure,
    #[error("unbalanced braces or tokens outside the program procedure")]
    UnbalancedBraces,
    #[error("procedure is not an operand of 'if' or 'ifelse'")]
    UnexpectedProcedure,
    #[error("unexpected delimiter '{}' in program", char::from(*.0))]
    UnexpectedDelimiter(u8),
    #[error("missing procedure block before 'if' operator")]
    MissingIfBlock,
    #[error("missing two procedure blocks before 'ifelse' operator")]
    MissingIfElseBlocks,
    #[error("invalid number literal: {0}")]
    InvalidNumber(String),
    #[error("stack underflow: needed {needed} elements, found {found}")]
    StackUnderflow { needed: usize, found: usize },
    #[error("division by zero")]
    DivisionByZero,
    #[error("negative sqrt")]
    NegativeSqrt,
    #[error("invalid log input: expected positive value, got {value}")]
    LogDomainError { value: f64 },
    #[error("invalid roll count n={n} larger than stack size {size}")]
    RollCountTooLarge { n: usize, size: usize },
    #[error("invalid copy count n={n} larger than stack size {size}")]
    CopyCountTooLarge { n: usize, size: usize },
    #[error("arithmetic overflow in {op} operation")]
    ArithmeticOverflow { op: &'static str },
    #[error("operand for {op} must be an integer (no fraction) within valid range, got {value}")]
    InvalidIntegerOperand { op: &'static str, value: f64 },
    #[error("operand for {op} must be a non-negative integer, got {value}")]
    NegativeIntegerOperand { op: &'static str, value: f64 },
    #[error("operand for {op} must be {expected}, got {found}")]
    InvalidOperandType {
        op: &'static str,
        expected: &'static str,
        found: &'static str,
    },
    #[error("instruction pointer overflow")]
    InstructionPointerOverflow,
    #[error("index out of bounds in {op} operation: length={length}, index={index}")]
    IndexOutOfBounds {
        op: &'static str,
        length: usize,
        index: usize,
    },
}

// An explicit frame stack eliminates recursion for executing nested procedure blocks.
struct Frame<'a> {
    ops: &'a [Operator],
    ip: usize,
}

/// The operand stack shared by the program and every procedure it executes.
struct OperandStack {
    values: Vec<Value>,
}

impl OperandStack {
    /// Pushes a value onto the top of the stack.
    fn push(&mut self, value: Value) -> Result<(), CalcError> {
        match value {
            Value::Real(real) if !real.is_finite() => {
                Err(CalcError::ArithmeticOverflow { op: "push" })
            }
            _ => {
                self.values.push(value);
                Ok(())
            }
        }
    }

    /// Pops a value from the top of the stack.
    /// Returns an error if the stack is empty.
    pub fn pop(&mut self) -> Result<Value, CalcError> {
        self.values.pop().ok_or(CalcError::StackUnderflow {
            needed: 1,
            found: 0,
        })
    }

    /// Returns the number of elements in the stack.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns the value at the top of the stack without removing it.
    /// Returns an error if the stack is empty.
    pub fn back(&self) -> Result<Value, CalcError> {
        self.values
            .last()
            .copied()
            .ok_or(CalcError::StackUnderflow {
                needed: 1,
                found: 0,
            })
    }
}

/// Executes a sequence of pre-parsed `Operator`s starting with `input_stack`.
///
/// The interpreter uses a typed operand stack. Procedures
/// (blocks for `if` / `ifelse`) are represented as nested `Vec<Operator>` and
/// operate on the same stack as the program that runs them.
///
/// Returned is the final stack contents (bottom-to-top order) on success.
///
/// Errors include stack underflow, division by zero, square root of a negative
/// number, and invalid counts for `roll` / `copy`.
pub fn execute(input_stack: Vec<Value>, ops: &[Operator]) -> Result<Vec<Value>, CalcError> {
    let mut stack = OperandStack {
        values: input_stack,
    };
    let mut frames: Vec<Frame> = Vec::new();
    frames.push(Frame { ops, ip: 0 });

    while let Some(frame) = frames.last_mut() {
        if frame.ip >= frame.ops.len() {
            frames.pop();
            continue;
        }

        let op = &frame
            .ops
            .get(frame.ip)
            .ok_or(CalcError::InstructionPointerOverflow)?;

        // Advance before executing (important for pushing child frames)
        frame.ip = frame
            .ip
            .checked_add(1)
            .ok_or(CalcError::ArithmeticOverflow { op: "ip_inc" })?;
        match op {
            Operator::Add => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(add(a, b)?)?;
            }
            Operator::Sub => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(sub(a, b)?)?;
            }
            Operator::Mul => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(mul(a, b)?)?;
            }
            Operator::Div => {
                let b = stack.pop()?;
                let b_num = b.to_f64("div")?;
                if b_num == 0.0 {
                    return Err(CalcError::DivisionByZero);
                }
                let a = stack.pop()?;
                let result = a.to_f64("div")? / b_num;
                stack.push(Value::Real(result))?;
            }
            Operator::Idiv => {
                let b = stack.pop()?.to_i32("idiv")?;
                if b == 0 {
                    return Err(CalcError::DivisionByZero);
                }
                let a = stack.pop()?.to_i32("idiv")?;
                stack.push(Value::Integer(
                    a.checked_div(b)
                        .ok_or(CalcError::ArithmeticOverflow { op: "idiv" })?,
                ))?;
            }
            Operator::Dup => {
                let a = stack.back()?;
                stack.push(a)?;
            }
            Operator::Exch => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(b)?;
                stack.push(a)?;
            }
            Operator::Pop => {
                stack.pop()?;
            }
            Operator::Eq => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(Value::Bool(a == b))?;
            }
            Operator::Ne => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(Value::Bool(a != b))?;
            }
            Operator::Gt => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(Value::Bool(a.to_f64("gt")? > b.to_f64("gt")?))?;
            }
            Operator::Lt => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(Value::Bool(a.to_f64("lt")? < b.to_f64("lt")?))?;
            }
            Operator::Ge => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(Value::Bool(a.to_f64("ge")? >= b.to_f64("ge")?))?;
            }
            Operator::Le => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(Value::Bool(a.to_f64("le")? <= b.to_f64("le")?))?;
            }
            Operator::And => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(logical_or_bitwise_binary(
                    "and",
                    a,
                    b,
                    |x, y| x && y,
                    |x, y| x & y,
                )?)?;
            }
            Operator::Or => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(logical_or_bitwise_binary(
                    "or",
                    a,
                    b,
                    |x, y| x || y,
                    |x, y| x | y,
                )?)?;
            }
            Operator::Xor => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(logical_or_bitwise_binary(
                    "xor",
                    a,
                    b,
                    |x, y| x ^ y,
                    |x, y| x ^ y,
                )?)?;
            }
            Operator::Not => {
                let a = stack.pop()?;
                stack.push(match a {
                    Value::Bool(value) => Value::Bool(!value),
                    Value::Integer(value) => Value::Integer(!value),
                    Value::Real(_) => {
                        return Err(CalcError::InvalidOperandType {
                            op: "not",
                            expected: "bool or integer",
                            found: a.type_name(),
                        });
                    }
                })?;
            }
            Operator::Bitshift => {
                let shift = stack.pop()?.to_i32("bitshift")?;
                let value = stack.pop()?.to_i32("bitshift")?;
                stack.push(Value::Integer(bitshift(value, shift)?))?;
            }
            Operator::If(block) => {
                let cond = stack.pop()?.to_bool("if")?;
                if cond {
                    frames.push(Frame { ops: block, ip: 0 });
                }
            }
            Operator::IfElse(block1, block2) => {
                let cond = stack.pop()?.to_bool("ifelse")?;
                let chosen = if cond { block1 } else { block2 };
                frames.push(Frame { ops: chosen, ip: 0 });
            }
            Operator::Copy => {
                let n_val = stack.pop()?.to_i32("copy")?;
                let n = usize::try_from(n_val).map_err(|_| CalcError::NegativeIntegerOperand {
                    op: "copy",
                    value: f64::from(n_val),
                })?;

                let start = stack
                    .len()
                    .checked_sub(n)
                    .ok_or(CalcError::ArithmeticOverflow { op: "copy_index" })?;
                // `start` is at most the length, and the copied values are already finite.
                stack.values.extend_from_within(start..);
            }
            Operator::Index => {
                let n_val = stack.pop()?.to_i32("index")?;
                let n = usize::try_from(n_val).map_err(|_| CalcError::NegativeIntegerOperand {
                    op: "index",
                    value: f64::from(n_val),
                })?;
                let len = stack.len();
                if n >= len {
                    return Err(CalcError::IndexOutOfBounds {
                        op: "index",
                        length: len,
                        index: n,
                    });
                }

                let target = len
                    .checked_sub(1)
                    .and_then(|value| value.checked_sub(n))
                    .ok_or(CalcError::ArithmeticOverflow { op: "index" })?;
                let value = *stack
                    .values
                    .get(target)
                    .ok_or(CalcError::IndexOutOfBounds {
                        op: "index",
                        length: len,
                        index: n,
                    })?;
                stack.push(value)?;
            }
            Operator::Sqrt => {
                let a = stack.pop()?.to_f64("sqrt")?;
                if a < 0.0 {
                    return Err(CalcError::NegativeSqrt);
                }
                stack.push(Value::Real(a.sqrt()))?;
            }
            Operator::Sin => {
                let angle = stack.pop()?.to_f64("sin")?;
                stack.push(Value::Real(angle.to_radians().sin()))?;
            }
            Operator::Cos => {
                let angle = stack.pop()?.to_f64("cos")?;
                stack.push(Value::Real(angle.to_radians().cos()))?;
            }
            Operator::Tan => {
                let angle = stack.pop()?.to_f64("tan")?;
                stack.push(Value::Real(angle.to_radians().tan()))?;
            }
            Operator::Atan => {
                let x = stack.pop()?.to_f64("atan")?;
                let y = stack.pop()?.to_f64("atan")?;
                let angle = y.atan2(x).to_degrees();
                // `atan2` yields (-180, 180]; PostScript angles lie in [0, 360).
                let angle = if angle < 0.0 { angle + 360.0 } else { angle };
                stack.push(Value::Real(angle))?;
            }
            Operator::Exp => {
                let exponent = stack.pop()?.to_f64("exp")?;
                let base = stack.pop()?.to_f64("exp")?;
                stack.push(checked_real(base.powf(exponent), "exp")?)?;
            }
            Operator::Ln => {
                let a = stack.pop()?.to_f64("ln")?;
                if a <= 0.0 {
                    return Err(CalcError::LogDomainError { value: a });
                }
                stack.push(Value::Real(a.ln()))?;
            }
            Operator::Log => {
                let a = stack.pop()?.to_f64("log")?;
                if a <= 0.0 {
                    return Err(CalcError::LogDomainError { value: a });
                }
                stack.push(Value::Real(a.log10()))?;
            }
            Operator::Mod => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(modulo(a, b)?)?;
            }
            Operator::Floor => {
                let a = stack.pop()?;
                stack.push(round_number(a, "floor", f64::floor)?)?;
            }
            Operator::Ceiling => {
                let a = stack.pop()?;
                stack.push(round_number(a, "ceiling", f64::ceil)?)?;
            }
            Operator::Round => {
                let a = stack.pop()?;
                stack.push(round_number(a, "round", |value| {
                    // Halves round toward positive infinity (`-2.5` becomes `-2`).
                    let floor = value.floor();
                    if value - floor >= 0.5 {
                        floor + 1.0
                    } else {
                        floor
                    }
                })?)?;
            }
            Operator::Cvi => {
                let a = stack.pop()?;
                let truncated = a.to_f64("cvi")?.trunc();
                stack.push(real_to_checked_integer(truncated, "cvi")?)?;
            }
            Operator::Cvr => {
                let a = stack.pop()?;
                stack.push(Value::Real(a.to_f64("cvr")?))?;
            }
            Operator::Abs => {
                let a = stack.pop()?;
                stack.push(abs(a)?)?;
            }
            Operator::Neg => {
                let a = stack.pop()?;
                stack.push(neg(a)?)?;
            }
            Operator::Roll => {
                let m_val = stack.pop()?.to_i32("roll")?;
                let n_val = stack.pop()?.to_i32("roll")?;
                let m = isize::try_from(m_val).map_err(|_| CalcError::NegativeIntegerOperand {
                    op: "roll",
                    value: f64::from(m_val),
                })?;
                let n = usize::try_from(n_val).map_err(|_| CalcError::InvalidIntegerOperand {
                    op: "roll",
                    value: f64::from(n_val),
                })?;
                if n > stack.len() {
                    return Err(CalcError::RollCountTooLarge {
                        n,
                        size: stack.len(),
                    });
                }
                if n == 0 {
                    // Nothing to rotate.
                    continue;
                }
                let start = stack
                    .len()
                    .checked_sub(n)
                    .ok_or(CalcError::ArithmeticOverflow { op: "roll_index" })?;
                let n_isize =
                    isize::try_from(n).map_err(|_| CalcError::ArithmeticOverflow { op: "roll" })?;
                // Normalize m into [0, n) using `rem_euclid` to handle negatives & large |m|.
                let m_norm = m.rem_euclid(n_isize);
                let m_norm_usize = usize::try_from(m_norm)
                    .map_err(|_| CalcError::ArithmeticOverflow { op: "roll" })?;
                if m_norm_usize != 0 {
                    let length = stack.len();
                    // Rotate only if there's an actual shift.
                    let tail =
                        stack
                            .values
                            .get_mut(start..)
                            .ok_or(CalcError::IndexOutOfBounds {
                                op: "roll",
                                length,
                                index: start,
                            })?;
                    tail.rotate_right(m_norm_usize);
                }
            }
            Operator::Truncate => {
                let a = stack.pop()?;
                stack.push(round_number(a, "truncate", f64::trunc)?)?;
            }
            Operator::Number(value) => stack.push(*value)?,
        }
    }

    // The root frame finished, so the stack holds the program result.
    Ok(stack.values)
}

/// Convenience helper that parses a Type 4 `program` (one `{ ... }` procedure)
/// with [`parse`] and then invokes [`execute`].
///
/// The `input_stack` supplies initial operands (in bottom-to-top order).
pub fn evaluate_postscript(input_stack: &[Value], program: &[u8]) -> Result<Vec<Value>, CalcError> {
    let ops = parse(program)?;
    execute(input_stack.to_vec(), &ops)
}

fn add(a: Value, b: Value) -> Result<Value, CalcError> {
    match (a, b) {
        (Value::Integer(a), Value::Integer(b)) => a
            .checked_add(b)
            .map(Value::Integer)
            .ok_or(CalcError::ArithmeticOverflow { op: "add" }),
        _ => checked_real(a.to_f64("add")? + b.to_f64("add")?, "add"),
    }
}

fn sub(a: Value, b: Value) -> Result<Value, CalcError> {
    match (a, b) {
        (Value::Integer(a), Value::Integer(b)) => a
            .checked_sub(b)
            .map(Value::Integer)
            .ok_or(CalcError::ArithmeticOverflow { op: "sub" }),
        _ => checked_real(a.to_f64("sub")? - b.to_f64("sub")?, "sub"),
    }
}

fn mul(a: Value, b: Value) -> Result<Value, CalcError> {
    match (a, b) {
        (Value::Integer(a), Value::Integer(b)) => a
            .checked_mul(b)
            .map(Value::Integer)
            .ok_or(CalcError::ArithmeticOverflow { op: "mul" }),
        _ => checked_real(a.to_f64("mul")? * b.to_f64("mul")?, "mul"),
    }
}

fn modulo(a: Value, b: Value) -> Result<Value, CalcError> {
    match (a, b) {
        (Value::Integer(a), Value::Integer(b)) => {
            if b == 0 {
                return Err(CalcError::DivisionByZero);
            }
            a.checked_rem(b)
                .map(Value::Integer)
                .ok_or(CalcError::ArithmeticOverflow { op: "mod" })
        }
        _ => {
            let b = b.to_f64("mod")?;
            if b == 0.0 {
                return Err(CalcError::DivisionByZero);
            }
            let a = a.to_f64("mod")?;
            checked_real(a % b, "mod")
        }
    }
}

fn abs(value: Value) -> Result<Value, CalcError> {
    match value {
        Value::Integer(value) => value
            .checked_abs()
            .map(Value::Integer)
            .ok_or(CalcError::ArithmeticOverflow { op: "abs" }),
        Value::Real(value) => checked_real(value.abs(), "abs"),
        Value::Bool(_) => Err(CalcError::InvalidOperandType {
            op: "abs",
            expected: "number",
            found: value.type_name(),
        }),
    }
}

fn neg(value: Value) -> Result<Value, CalcError> {
    match value {
        Value::Integer(value) => Ok(value
            .checked_neg()
            .map_or_else(|| Value::Real(-f64::from(value)), Value::Integer)),
        Value::Real(value) => checked_real(-value, "neg"),
        Value::Bool(_) => Err(CalcError::InvalidOperandType {
            op: "neg",
            expected: "number",
            found: value.type_name(),
        }),
    }
}

/// Applies an integral rounding function to a number, keeping the operand's type.
fn round_number(value: Value, op: &'static str, round: fn(f64) -> f64) -> Result<Value, CalcError> {
    match value {
        Value::Integer(_) => Ok(value),
        Value::Real(value) => checked_real(round(value), op),
        Value::Bool(_) => Err(CalcError::InvalidOperandType {
            op,
            expected: "number",
            found: value.type_name(),
        }),
    }
}

fn logical_or_bitwise_binary(
    op: &'static str,
    a: Value,
    b: Value,
    bool_op: fn(bool, bool) -> bool,
    int_op: fn(i32, i32) -> i32,
) -> Result<Value, CalcError> {
    match (a, b) {
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(bool_op(a, b))),
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(int_op(a, b))),
        _ => Err(CalcError::InvalidOperandType {
            op,
            expected: "matching bool or integer operands",
            found: "mixed operands",
        }),
    }
}

fn bitshift(value: i32, shift: i32) -> Result<i32, CalcError> {
    if shift >= 0 {
        let amount =
            u32::try_from(shift).map_err(|_| CalcError::ArithmeticOverflow { op: "bitshift" })?;
        value
            .checked_shl(amount)
            .ok_or(CalcError::ArithmeticOverflow { op: "bitshift" })
    } else {
        let amount = shift
            .checked_neg()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(CalcError::ArithmeticOverflow { op: "bitshift" })?;
        value
            .checked_shr(amount)
            .ok_or(CalcError::ArithmeticOverflow { op: "bitshift" })
    }
}

fn real_to_checked_integer(value: f64, op: &'static str) -> Result<Value, CalcError> {
    let int = value
        .to_i32()
        .ok_or(CalcError::InvalidIntegerOperand { op, value })?;
    Ok(Value::Integer(int))
}

fn checked_real(value: f64, op: &'static str) -> Result<Value, CalcError> {
    if value.is_finite() {
        Ok(Value::Real(value))
    } else {
        Err(CalcError::ArithmeticOverflow { op })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod typed_tests {
    use super::*;

    fn assert_real_approx_eq(value: Value, expected: f64) {
        let Value::Real(actual) = value else {
            panic!("expected real value, got {value:?}");
        };
        let delta = (actual - expected).abs();
        assert!(
            delta < 1e-12,
            "expected {expected}, got {actual}, delta {delta}"
        );
    }

    #[test]
    fn parses_typed_literals_and_new_operators() {
        let ops = parse(b"{ 1 2.5 -3 true false idiv bitshift }").unwrap();
        assert_eq!(
            ops,
            vec![
                Operator::Number(Value::Integer(1)),
                Operator::Number(Value::Real(2.5)),
                Operator::Number(Value::Integer(-3)),
                Operator::Number(Value::Bool(true)),
                Operator::Number(Value::Bool(false)),
                Operator::Idiv,
                Operator::Bitshift,
            ]
        );
    }

    #[test]
    fn integer_arithmetic_preserves_integer_type() {
        let result = evaluate_postscript(&[], b"{ 2 3 add 4 mul 5 sub }").unwrap();
        assert_eq!(result, vec![Value::Integer(15)]);
    }

    #[test]
    fn mixed_arithmetic_promotes_to_real() {
        let result = evaluate_postscript(&[], b"{ 2 3.5 add }").unwrap();
        assert_eq!(result, vec![Value::Real(5.5)]);
    }

    #[test]
    fn div_always_returns_real() {
        let result = evaluate_postscript(&[], b"{ 8 2 div }").unwrap();
        assert_eq!(result, vec![Value::Real(4.0)]);
    }

    #[test]
    fn idiv_truncates_toward_zero() {
        let result = evaluate_postscript(&[], b"{ 7 3 idiv -7 3 idiv 7 -3 idiv }").unwrap();
        assert_eq!(
            result,
            vec![Value::Integer(2), Value::Integer(-2), Value::Integer(-2)]
        );
    }

    #[test]
    fn idiv_rejects_zero_and_real_operands() {
        let err = evaluate_postscript(&[], b"{ 1 0 idiv }").unwrap_err();
        assert_eq!(err, CalcError::DivisionByZero);

        let err = evaluate_postscript(&[], b"{ 1.0 1 idiv }").unwrap_err();
        assert!(matches!(
            err,
            CalcError::InvalidIntegerOperand {
                op: "idiv",
                value: 1.0
            }
        ));
    }

    #[test]
    fn bitshift_shifts_left_and_arithmetic_right() {
        let result = evaluate_postscript(&[], b"{ 3 2 bitshift -8 -1 bitshift }").unwrap();
        assert_eq!(result, vec![Value::Integer(12), Value::Integer(-4)]);
    }

    #[test]
    fn bitshift_rejects_invalid_shift_amounts() {
        let err = evaluate_postscript(&[], b"{ 1 32 bitshift }").unwrap_err();
        assert_eq!(err, CalcError::ArithmeticOverflow { op: "bitshift" });
    }

    #[test]
    fn boolean_control_flow_uses_bool_values() {
        let result = evaluate_postscript(&[], b"{ true { 2 3 add } if }").unwrap();
        assert_eq!(result, vec![Value::Integer(5)]);

        let result = evaluate_postscript(&[], b"{ false { 2 } { 3 } ifelse }").unwrap();
        assert_eq!(result, vec![Value::Integer(3)]);
    }

    #[test]
    fn control_flow_rejects_numeric_condition() {
        let err = evaluate_postscript(&[], b"{ 1 { 2 } if }").unwrap_err();
        assert!(matches!(
            err,
            CalcError::InvalidOperandType {
                op: "if",
                expected: "bool",
                found: "integer"
            }
        ));
    }

    #[test]
    fn comparisons_and_logical_ops_return_bools() {
        let result = evaluate_postscript(&[], b"{ 2 3 lt true xor }").unwrap();
        assert_eq!(result, vec![Value::Bool(false)]);
    }

    #[test]
    fn integer_bitwise_ops_use_integer_type() {
        let result = evaluate_postscript(&[], b"{ 6 3 and 4 or 7 xor not }").unwrap();
        assert_eq!(result, vec![Value::Integer(-2)]);
    }

    #[test]
    fn stack_ops_accept_integer_counts() {
        let result = evaluate_postscript(&[], b"{ 1 2 3 2 copy 4 5 3 1 roll }").unwrap();
        assert_eq!(
            result,
            vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
                Value::Integer(2),
                Value::Integer(5),
                Value::Integer(3),
                Value::Integer(4),
            ]
        );
    }

    #[test]
    fn cvi_cvr_floor_truncate_and_abs_convert_types() {
        let result =
            evaluate_postscript(&[], b"{ 3.7 cvi -2.1 floor 5 cvr -2.9 truncate }").unwrap();
        assert_eq!(
            result,
            vec![
                Value::Integer(3),
                Value::Real(-3.0),
                Value::Real(5.0),
                Value::Real(-2.0),
            ]
        );

        let result = evaluate_postscript(&[], b"{ -5 abs 3.25 abs }").unwrap();
        assert_eq!(result, vec![Value::Integer(5), Value::Real(3.25)]);
    }

    #[test]
    fn transcendental_ops_still_return_reals() {
        let result = evaluate_postscript(&[], b"{ 9 sqrt 30 sin }").unwrap();
        assert_real_approx_eq(result[0], 3.0);
        assert_real_approx_eq(result[1], 0.5);
    }
}
