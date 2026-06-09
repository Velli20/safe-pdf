use crate::{operator::Operator, parser::parse_tokens, value::Value};
use num_traits::ToPrimitive;
use thiserror::Error;

/// Errors that can occur while executing a PostScript-like calculator program.
#[derive(Debug, Error, PartialEq)]
pub enum CalcError {
    #[error("unexpected end of block stack")]
    EmptyBlockStack,
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
    #[error("token index overflow while parsing")]
    TokenIndexOverflow,
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
    stack: Vec<Value>,
}

impl<'a> Frame<'a> {
    /// Handles completion of a frame: propagates result to parent or returns if root.
    /// Returns Some(final_stack) if execution should return, or None to continue.
    fn complete_frame(frames: &mut Vec<Frame<'a>>) -> Option<Vec<Value>> {
        let finished = frames.pop()?;
        if let Some(parent) = frames.last_mut() {
            parent.stack.clear();
            parent.stack.extend(finished.stack);
            None
        } else {
            Some(finished.stack)
        }
    }
}

impl Frame<'_> {
    /// Pushes a value onto the top of the stack.
    fn push(&mut self, value: Value) -> Result<(), CalcError> {
        match value {
            Value::Real(real) if !real.is_finite() => {
                Err(CalcError::ArithmeticOverflow { op: "push" })
            }
            _ => {
                self.stack.push(value);
                Ok(())
            }
        }
    }

    /// Pops a value from the top of the stack.
    /// Returns an error if the stack is empty.
    pub fn pop(&mut self) -> Result<Value, CalcError> {
        self.stack.pop().ok_or(CalcError::StackUnderflow {
            needed: 1,
            found: 0,
        })
    }

    /// Returns the number of elements in the stack.
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Returns the value at the top of the stack without removing it.
    /// Returns an error if the stack is empty.
    pub fn back(&self) -> Result<Value, CalcError> {
        self.stack.last().copied().ok_or(CalcError::StackUnderflow {
            needed: 1,
            found: 0,
        })
    }
}

/// Executes a sequence of pre-parsed `Operator`s starting with `input_stack`.
///
/// The interpreter uses a typed operand stack. Procedures
/// (blocks for `if` / `ifelse`) are represented as nested `Vec<Operator>` and
/// are executed with cloned snapshots of the current stack.
///
/// Returned is the final stack contents (bottom-to-top order) on success.
///
/// Errors include stack underflow, division by zero, square root of a negative
/// number, and invalid counts for `roll` / `copy`.
pub fn execute(input_stack: &[Value], ops: &[Operator]) -> Result<Vec<Value>, CalcError> {
    let mut frames: Vec<Frame> = Vec::new();
    frames.push(Frame {
        ops,
        ip: 0,
        stack: Vec::from(input_stack),
    });

    while let Some(frame) = frames.last_mut() {
        if frame.ip >= frame.ops.len() {
            if let Some(final_stack) = Frame::complete_frame(&mut frames) {
                return Ok(final_stack);
            } else {
                continue;
            }
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
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(add(a, b)?)?;
            }
            Operator::Sub => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(sub(a, b)?)?;
            }
            Operator::Mul => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(mul(a, b)?)?;
            }
            Operator::Div => {
                let b = frame.pop()?;
                let b_num = b.to_f64("div")?;
                if b_num == 0.0 {
                    return Err(CalcError::DivisionByZero);
                }
                let a = frame.pop()?;
                let result = a.to_f64("div")? / b_num;
                frame.push(Value::Real(result))?;
            }
            Operator::Idiv => {
                let b = frame.pop()?.to_i32("idiv")?;
                if b == 0 {
                    return Err(CalcError::DivisionByZero);
                }
                let a = frame.pop()?.to_i32("idiv")?;
                frame.push(Value::Integer(
                    a.checked_div(b)
                        .ok_or(CalcError::ArithmeticOverflow { op: "idiv" })?,
                ))?;
            }
            Operator::Dup => {
                let a = frame.back()?;
                frame.push(a)?;
            }
            Operator::Exch => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(b)?;
                frame.push(a)?;
            }
            Operator::Pop => {
                frame.pop()?;
            }
            Operator::Eq => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(Value::Bool(a == b))?;
            }
            Operator::Ne => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(Value::Bool(a != b))?;
            }
            Operator::Gt => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(Value::Bool(a.to_f64("gt")? > b.to_f64("gt")?))?;
            }
            Operator::Lt => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(Value::Bool(a.to_f64("lt")? < b.to_f64("lt")?))?;
            }
            Operator::Ge => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(Value::Bool(a.to_f64("ge")? >= b.to_f64("ge")?))?;
            }
            Operator::Le => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(Value::Bool(a.to_f64("le")? <= b.to_f64("le")?))?;
            }
            Operator::And => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(logical_or_bitwise_binary(
                    "and",
                    a,
                    b,
                    |x, y| x && y,
                    |x, y| x & y,
                )?)?;
            }
            Operator::Or => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(logical_or_bitwise_binary(
                    "or",
                    a,
                    b,
                    |x, y| x || y,
                    |x, y| x | y,
                )?)?;
            }
            Operator::Xor => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(logical_or_bitwise_binary(
                    "xor",
                    a,
                    b,
                    |x, y| x ^ y,
                    |x, y| x ^ y,
                )?)?;
            }
            Operator::Not => {
                let a = frame.pop()?;
                frame.push(match a {
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
                let shift = frame.pop()?.to_i32("bitshift")?;
                let value = frame.pop()?.to_i32("bitshift")?;
                frame.push(Value::Integer(bitshift(value, shift)?))?;
            }
            Operator::If(block) => {
                let cond = frame.pop()?.to_bool("if")?;
                if cond {
                    // Push new frame with a cloned snapshot of current stack
                    let snapshot = frame.stack.clone();
                    frames.push(Frame {
                        ops: block,
                        ip: 0,
                        stack: snapshot,
                    });
                }
            }
            Operator::IfElse(block1, block2) => {
                let cond = frame.pop()?.to_bool("ifelse")?;
                let chosen = if cond { block1 } else { block2 };
                let snapshot = frame.stack.clone();
                frames.push(Frame {
                    ops: chosen,
                    ip: 0,
                    stack: snapshot,
                });
            }
            Operator::Copy => {
                let n_val = frame.pop()?.to_i32("copy")?;
                let n = usize::try_from(n_val).map_err(|_| CalcError::NegativeIntegerOperand {
                    op: "copy",
                    value: f64::from(n_val),
                })?;

                let len = frame.len();
                let start = len
                    .checked_sub(n)
                    .ok_or(CalcError::ArithmeticOverflow { op: "copy_index" })?;
                let to_copy = frame
                    .stack
                    .get(start..)
                    .ok_or(CalcError::IndexOutOfBounds {
                        op: "copy",
                        length: frame.len(),
                        index: start,
                    })?
                    .to_vec();
                for v in to_copy {
                    frame.push(v)?;
                }
            }
            Operator::Index => {
                let n_val = frame.pop()?.to_i32("index")?;
                let n = usize::try_from(n_val).map_err(|_| CalcError::NegativeIntegerOperand {
                    op: "index",
                    value: f64::from(n_val),
                })?;
                let len = frame.len();
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
                let value = *frame.stack.get(target).ok_or(CalcError::IndexOutOfBounds {
                    op: "index",
                    length: len,
                    index: n,
                })?;
                frame.push(value)?;
            }
            Operator::Sqrt => {
                let a = frame.pop()?.to_f64("sqrt")?;
                if a < 0.0 {
                    return Err(CalcError::NegativeSqrt);
                }
                frame.push(Value::Real(a.sqrt()))?;
            }
            Operator::Sin => {
                let angle = frame.pop()?.to_f64("sin")?;
                frame.push(Value::Real(angle.to_radians().sin()))?;
            }
            Operator::Cos => {
                let angle = frame.pop()?.to_f64("cos")?;
                frame.push(Value::Real(angle.to_radians().cos()))?;
            }
            Operator::Tan => {
                let angle = frame.pop()?.to_f64("tan")?;
                frame.push(Value::Real(angle.to_radians().tan()))?;
            }
            Operator::Atan => {
                let x = frame.pop()?.to_f64("atan")?;
                let y = frame.pop()?.to_f64("atan")?;
                frame.push(Value::Real(y.atan2(x).to_degrees()))?;
            }
            Operator::Log => {
                let a = frame.pop()?.to_f64("log")?;
                if a <= 0.0 {
                    return Err(CalcError::LogDomainError { value: a });
                }
                frame.push(Value::Real(a.ln()))?;
            }
            Operator::Mod => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(modulo(a, b)?)?;
            }
            Operator::Floor => {
                let a = frame.pop()?.to_f64("floor")?;
                frame.push(real_to_checked_integer(a.floor(), "floor")?)?;
            }
            Operator::Cvi => {
                let a = frame.pop()?;
                let truncated = a.to_f64("cvi")?.trunc();
                frame.push(real_to_checked_integer(truncated, "cvi")?)?;
            }
            Operator::Cvr => {
                let a = frame.pop()?;
                frame.push(Value::Real(a.to_f64("cvr")?))?;
            }
            Operator::Abs => {
                let a = frame.pop()?;
                frame.push(abs(a)?)?;
            }
            Operator::Roll => {
                let m_val = frame.pop()?.to_i32("roll")?;
                let n_val = frame.pop()?.to_i32("roll")?;
                let m = isize::try_from(m_val).map_err(|_| CalcError::NegativeIntegerOperand {
                    op: "roll",
                    value: f64::from(m_val),
                })?;
                let n = usize::try_from(n_val).map_err(|_| CalcError::InvalidIntegerOperand {
                    op: "roll",
                    value: f64::from(n_val),
                })?;
                if n > frame.len() {
                    return Err(CalcError::RollCountTooLarge {
                        n,
                        size: frame.len(),
                    });
                }
                if n == 0 {
                    // Nothing to rotate.
                    continue;
                }
                let start = frame
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
                    let length = frame.len();
                    // Rotate only if there's an actual shift.
                    let tail = frame
                        .stack
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
                let a = frame.pop()?.to_f64("truncate")?;
                frame.push(real_to_checked_integer(a.trunc(), "truncate")?)?;
            }
            Operator::Number(value) => frame.push(*value)?,
        }
    }

    // Should only be reachable if there were zero frames (which cannot happen)
    Ok(Vec::new())
}

/// Convenience helper that tokenizes & parses a PostScript-like `code` string
/// and then invokes [`execute`].
///
/// The `input_stack` supplies initial operands (in bottom-to-top order). The
/// `code` string can contain numeric literals, the supported operators, and
/// procedure blocks delimited by `{` and `}` used by `if` / `ifelse`.
pub fn evaluate_postscript(input_stack: &[Value], code: &str) -> Result<Vec<Value>, CalcError> {
    let code = code.replace("{", " { ").replace("}", " } ");
    let ops = parse_tokens(&code.split_whitespace().collect::<Vec<_>>())?;
    execute(input_stack, &ops)
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

#[cfg(all(test, not(test)))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn assert_approx_eq(actual: f64, expected: f64) {
        let delta = (actual - expected).abs();
        assert!(
            delta < 1e-12,
            "expected {expected}, got {actual}, delta {delta}"
        );
    }

    #[test]
    fn test_parse_simple_operators() {
        let tokens = vec!["add", "sub", "mul", "div"];
        let ops = parse_tokens(&tokens).unwrap();
        assert_eq!(
            ops,
            vec![Operator::Add, Operator::Sub, Operator::Mul, Operator::Div]
        );
    }

    #[test]
    fn test_parse_numbers() {
        let tokens = vec!["1", "2.5", "-3"];
        let ops = parse_tokens(&tokens).unwrap();
        assert_eq!(
            ops,
            vec![
                Operator::Number(1.0),
                Operator::Number(2.5),
                Operator::Number(-3.0)
            ]
        );
    }

    #[test]
    fn test_parse_if_block() {
        let tokens = vec!["{", "2", "3", "add", "}", "if"];
        let ops = parse_tokens(&tokens).unwrap();
        assert_eq!(
            ops,
            vec![Operator::If(vec![
                Operator::Number(2.0),
                Operator::Number(3.0),
                Operator::Add
            ])]
        );
    }

    #[test]
    fn test_parse_ifelse_block() {
        let tokens = vec![
            "{", "2", "3", "add", "}", "{", "4", "5", "add", "}", "ifelse",
        ];
        let ops = parse_tokens(&tokens).unwrap();
        assert_eq!(
            ops,
            vec![Operator::IfElse(
                vec![Operator::Number(2.0), Operator::Number(3.0), Operator::Add],
                vec![Operator::Number(4.0), Operator::Number(5.0), Operator::Add]
            )]
        );
    }

    #[test]
    fn test_parse_nested_blocks() {
        let tokens = vec!["{", "1", "{", "2", "3", "add", "}", "if", "}", "if"];
        let ops = parse_tokens(&tokens).unwrap();
        assert_eq!(
            ops,
            vec![Operator::If(vec![
                Operator::Number(1.0),
                Operator::If(vec![
                    Operator::Number(2.0),
                    Operator::Number(3.0),
                    Operator::Add
                ])
            ])]
        );
    }

    #[test]
    fn test_parse_invalid_number() {
        let tokens = vec!["foo"];
        let err = parse_tokens(&tokens).unwrap_err();
        assert!(matches!(err, CalcError::InvalidNumber(_)));
    }

    #[test]
    fn test_parse_logical_operators() {
        let tokens = vec![
            "eq", "ne", "gt", "lt", "ge", "le", "and", "or", "xor", "not",
        ];
        let ops = parse_tokens(&tokens).unwrap();
        assert_eq!(
            ops,
            vec![
                Operator::Eq,
                Operator::Ne,
                Operator::Gt,
                Operator::Lt,
                Operator::Ge,
                Operator::Le,
                Operator::And,
                Operator::Or,
                Operator::Xor,
                Operator::Not
            ]
        );
    }

    #[test]
    fn test_parse_transcendental_operators() {
        let tokens = vec!["sin", "cos", "tan", "atan", "log", "floor"];
        let ops = parse_tokens(&tokens).unwrap();
        assert_eq!(
            ops,
            vec![
                Operator::Sin,
                Operator::Cos,
                Operator::Tan,
                Operator::Atan,
                Operator::Log,
                Operator::Floor
            ]
        );
    }

    #[test]
    fn test_parse_cvr_operator() {
        let tokens = vec!["cvr"];
        let ops = parse_tokens(&tokens).unwrap();
        assert_eq!(ops, vec![Operator::Cvr]);
    }

    #[test]
    fn test_add() {
        let result = evaluate_postscript(&[2.0, 3.0], "add").unwrap();
        assert_eq!(result, vec![5.0]);
    }

    #[test]
    fn test_sub() {
        let result = evaluate_postscript(&[5.0, 2.0], "sub").unwrap();
        assert_eq!(result, vec![3.0]);
    }

    #[test]
    fn test_mul() {
        let result = evaluate_postscript(&[4.0, 3.0], "mul").unwrap();
        assert_eq!(result, vec![12.0]);
    }

    #[test]
    fn test_div() {
        let result = evaluate_postscript(&[8.0, 2.0], "div").unwrap();
        assert_eq!(result, vec![4.0]);
    }

    #[test]
    fn test_cvr_is_no_op_for_real_stack_values() {
        let result = evaluate_postscript(&[8.5], "cvr").unwrap();
        assert_eq!(result, vec![8.5]);
    }

    #[test]
    fn test_dup() {
        let result = evaluate_postscript(&[7.0], "dup").unwrap();
        assert_eq!(result, vec![7.0, 7.0]);
    }

    #[test]
    fn test_exch() {
        let result = evaluate_postscript(&[1.0, 2.0], "exch").unwrap();
        assert_eq!(result, vec![2.0, 1.0]);
    }

    #[test]
    fn test_pop() {
        let result = evaluate_postscript(&[1.0, 2.0, 3.0], "pop").unwrap();
        assert_eq!(result, vec![1.0, 2.0]);
    }

    #[test]
    fn test_eq() {
        let result = evaluate_postscript(&[2.0, 2.0], "eq").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[2.0, 3.0], "eq").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_ne() {
        let result = evaluate_postscript(&[2.0, 3.0], "ne").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[2.0, 2.0], "ne").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_gt() {
        let result = evaluate_postscript(&[3.0, 2.0], "gt").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[2.0, 3.0], "gt").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_lt() {
        let result = evaluate_postscript(&[2.0, 3.0], "lt").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[3.0, 2.0], "lt").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_ge() {
        let result = evaluate_postscript(&[3.0, 2.0], "ge").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[2.0, 2.0], "ge").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[1.0, 2.0], "ge").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_le() {
        let result = evaluate_postscript(&[2.0, 3.0], "le").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[2.0, 2.0], "le").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[3.0, 2.0], "le").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_and() {
        let result = evaluate_postscript(&[1.0, 1.0], "and").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[1.0, 0.0], "and").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_or() {
        let result = evaluate_postscript(&[0.0, 1.0], "or").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[0.0, 0.0], "or").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_xor() {
        let result = evaluate_postscript(&[1.0, 0.0], "xor").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[1.0, 1.0], "xor").unwrap();
        assert_eq!(result, vec![0.0]);
        let result = evaluate_postscript(&[0.0, 0.0], "xor").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_not() {
        let result = evaluate_postscript(&[0.0], "not").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[1.0], "not").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_if_true() {
        let result = evaluate_postscript(&[1.0], "{ 2 3 add } if").unwrap();
        assert_eq!(result, vec![5.0]);
    }

    #[test]
    fn test_if_false() {
        let result = evaluate_postscript(&[0.0], "{ 2 3 add } if").unwrap();
        assert_eq!(result, Vec::<f64>::new());
    }

    #[test]
    fn test_ifelse_true() {
        let result = evaluate_postscript(&[1.0], "{ 2 3 add } { 4 5 add } ifelse").unwrap();
        assert_eq!(result, vec![5.0]);
    }

    #[test]
    fn test_ifelse_false() {
        let result = evaluate_postscript(&[0.0], "{ 2 3 add } { 4 5 add } ifelse").unwrap();
        assert_eq!(result, vec![9.0]);
    }

    #[test]
    fn test_nested_blocks() {
        let result = evaluate_postscript(&[1.0], "{ 1 { 2 3 add } if } if").unwrap();
        assert_eq!(result, vec![5.0]);
    }

    #[test]
    fn test_complex_expression() {
        let result = evaluate_postscript(&[2.0, 3.0, 4.0], "add mul").unwrap();
        assert_eq!(result, vec![14.0]);
    }

    #[test]
    fn test_copy() {
        let result = evaluate_postscript(&[1.0, 2.0, 3.0], "2 copy").unwrap();
        assert_eq!(result, vec![1.0, 2.0, 3.0, 2.0, 3.0]);
    }

    #[test]
    fn test_index() {
        let result = evaluate_postscript(&[1.0, 2.0, 3.0, 4.0], "2 index").unwrap();
        assert_eq!(result, vec![1.0, 2.0, 3.0, 4.0, 2.0]);
    }

    #[test]
    fn test_roll() {
        let result = evaluate_postscript(&[1.0, 2.0, 3.0, 4.0, 5.0], "3 1 roll").unwrap();
        assert_eq!(result, vec![1.0, 2.0, 5.0, 3.0, 4.0]);
        let result = evaluate_postscript(&[1.0, 2.0, 3.0, 4.0, 5.0], "4 -2 roll").unwrap();
        assert_eq!(result, vec![1.0, 4.0, 5.0, 2.0, 3.0]);
        let result = evaluate_postscript(&[1.0, 2.0, 3.0, 4.0, 5.0], "0 7 roll").unwrap();
        assert_eq!(result, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_sqrt() {
        let result = evaluate_postscript(&[9.0], "sqrt").unwrap();
        assert_eq!(result, vec![3.0]);
        let result = evaluate_postscript(&[2.25], "sqrt").unwrap();
        assert_eq!(result, vec![1.5]);
    }

    #[test]
    fn test_sin() {
        let result = evaluate_postscript(&[30.0], "sin").unwrap();
        assert_approx_eq(result[0], 0.5);
        let result = evaluate_postscript(&[90.0], "sin").unwrap();
        assert_approx_eq(result[0], 1.0);
    }

    #[test]
    fn test_cos() {
        let result = evaluate_postscript(&[60.0], "cos").unwrap();
        assert_approx_eq(result[0], 0.5);
        let result = evaluate_postscript(&[180.0], "cos").unwrap();
        assert_approx_eq(result[0], -1.0);
    }

    #[test]
    fn test_tan() {
        let result = evaluate_postscript(&[45.0], "tan").unwrap();
        assert_approx_eq(result[0], 1.0);
    }

    #[test]
    fn test_atan() {
        let result = evaluate_postscript(&[1.0, 1.0], "atan").unwrap();
        assert_approx_eq(result[0], 45.0);
        let result = evaluate_postscript(&[1.0, 0.0], "atan").unwrap();
        assert_approx_eq(result[0], 90.0);
    }

    #[test]
    fn test_log() {
        let result = evaluate_postscript(&[std::f64::consts::E], "log").unwrap();
        assert_approx_eq(result[0], 1.0);
        let result = evaluate_postscript(&[1.0], "log").unwrap();
        assert_approx_eq(result[0], 0.0);
    }

    #[test]
    fn test_log_domain_error() {
        let err = evaluate_postscript(&[0.0], "log").unwrap_err();
        assert!(matches!(err, CalcError::LogDomainError { value } if value == 0.0));
        let err = evaluate_postscript(&[-1.0], "log").unwrap_err();
        assert!(matches!(err, CalcError::LogDomainError { value } if value == -1.0));
    }

    #[test]
    fn test_floor() {
        let result = evaluate_postscript(&[3.7], "floor").unwrap();
        assert_eq!(result, vec![3.0]);
        let result = evaluate_postscript(&[-2.1], "floor").unwrap();
        assert_eq!(result, vec![-3.0]);
    }

    #[test]
    fn test_truncate() {
        let result = evaluate_postscript(&[3.7], "truncate").unwrap();
        assert_eq!(result, vec![3.0]);
        let result = evaluate_postscript(&[-2.9], "truncate").unwrap();
        assert_eq!(result, vec![-2.0]);
        let result = evaluate_postscript(&[0.0], "truncate").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_abs() {
        let result = evaluate_postscript(&[-5.0], "abs").unwrap();
        assert_eq!(result, vec![5.0]);
        let result = evaluate_postscript(&[3.2], "abs").unwrap();
        assert_eq!(result, vec![3.2]);
        let result = evaluate_postscript(&[0.0], "abs").unwrap();
        assert_eq!(result, vec![0.0]);
    }

    #[test]
    fn test_cvi() {
        let result = evaluate_postscript(&[3.7], "cvi").unwrap();
        assert_eq!(result, vec![3.0]);
        let result = evaluate_postscript(&[-2.9], "cvi").unwrap();
        assert_eq!(result, vec![-2.0]);
        let result = evaluate_postscript(&[0.0], "cvi").unwrap();
        assert_eq!(result, vec![0.0]);
        let result = evaluate_postscript(&[5.0], "cvi").unwrap();
        assert_eq!(result, vec![5.0]);
    }

    #[test]
    fn test_mod() {
        let result = evaluate_postscript(&[10.0, 3.0], "mod").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[-10.0, 3.0], "mod").unwrap();
        assert_eq!(result, vec![-1.0]);
        let result = evaluate_postscript(&[10.0, -3.0], "mod").unwrap();
        assert_eq!(result, vec![1.0]);
        let result = evaluate_postscript(&[0.0, 3.0], "mod").unwrap();
        assert_eq!(result, vec![0.0]);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
        let tokens = vec!["1", "2.5", "-3", "true", "false", "idiv", "bitshift"];
        let ops = parse_tokens(&tokens).unwrap();
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
        let result = evaluate_postscript(&[], "2 3 add 4 mul 5 sub").unwrap();
        assert_eq!(result, vec![Value::Integer(15)]);
    }

    #[test]
    fn mixed_arithmetic_promotes_to_real() {
        let result = evaluate_postscript(&[], "2 3.5 add").unwrap();
        assert_eq!(result, vec![Value::Real(5.5)]);
    }

    #[test]
    fn div_always_returns_real() {
        let result = evaluate_postscript(&[], "8 2 div").unwrap();
        assert_eq!(result, vec![Value::Real(4.0)]);
    }

    #[test]
    fn idiv_truncates_toward_zero() {
        let result = evaluate_postscript(&[], "7 3 idiv -7 3 idiv 7 -3 idiv").unwrap();
        assert_eq!(
            result,
            vec![Value::Integer(2), Value::Integer(-2), Value::Integer(-2)]
        );
    }

    #[test]
    fn idiv_rejects_zero_and_real_operands() {
        let err = evaluate_postscript(&[], "1 0 idiv").unwrap_err();
        assert_eq!(err, CalcError::DivisionByZero);

        let err = evaluate_postscript(&[], "1.0 1 idiv").unwrap_err();
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
        let result = evaluate_postscript(&[], "3 2 bitshift -8 -1 bitshift").unwrap();
        assert_eq!(result, vec![Value::Integer(12), Value::Integer(-4)]);
    }

    #[test]
    fn bitshift_rejects_invalid_shift_amounts() {
        let err = evaluate_postscript(&[], "1 32 bitshift").unwrap_err();
        assert_eq!(err, CalcError::ArithmeticOverflow { op: "bitshift" });
    }

    #[test]
    fn boolean_control_flow_uses_bool_values() {
        let result = evaluate_postscript(&[], "true { 2 3 add } if").unwrap();
        assert_eq!(result, vec![Value::Integer(5)]);

        let result = evaluate_postscript(&[], "false { 2 } { 3 } ifelse").unwrap();
        assert_eq!(result, vec![Value::Integer(3)]);
    }

    #[test]
    fn control_flow_rejects_numeric_condition() {
        let err = evaluate_postscript(&[], "1 { 2 } if").unwrap_err();
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
        let result = evaluate_postscript(&[], "2 3 lt true xor").unwrap();
        assert_eq!(result, vec![Value::Bool(false)]);
    }

    #[test]
    fn integer_bitwise_ops_use_integer_type() {
        let result = evaluate_postscript(&[], "6 3 and 4 or 7 xor not").unwrap();
        assert_eq!(result, vec![Value::Integer(-2)]);
    }

    #[test]
    fn stack_ops_accept_integer_counts() {
        let result = evaluate_postscript(&[], "1 2 3 2 copy 4 5 3 1 roll").unwrap();
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
        let result = evaluate_postscript(&[], "3.7 cvi -2.1 floor 5 cvr -2.9 truncate").unwrap();
        assert_eq!(
            result,
            vec![
                Value::Integer(3),
                Value::Integer(-3),
                Value::Real(5.0),
                Value::Integer(-2),
            ]
        );

        let result = evaluate_postscript(&[], "-5 abs 3.25 abs").unwrap();
        assert_eq!(result, vec![Value::Integer(5), Value::Real(3.25)]);
    }

    #[test]
    fn transcendental_ops_still_return_reals() {
        let result = evaluate_postscript(&[], "9 sqrt 30 sin").unwrap();
        assert_real_approx_eq(result[0], 3.0);
        assert_real_approx_eq(result[1], 0.5);
    }
}
