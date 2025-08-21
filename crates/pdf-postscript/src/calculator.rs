use crate::{operator::Operator, parser::parse_tokens};
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
}

// An explicit frame stack eliminates recursion for executing nested procedure blocks.
struct Frame<'a> {
    ops: &'a [Operator],
    ip: usize,
    stack: Vec<f64>,
}

impl<'a> Frame<'a> {
    /// Handles completion of a frame: propagates result to parent or returns if root.
    /// Returns Some(final_stack) if execution should return, or None to continue.
    fn complete_frame(frames: &mut Vec<Frame<'a>>) -> Option<Vec<f64>> {
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
    fn push(&mut self, value: f64) -> Result<(), CalcError> {
        if value.is_finite() {
            self.stack.push(value);
            Ok(())
        } else {
            Err(CalcError::ArithmeticOverflow { op: "push" })
        }
    }

    /// Pops a value from the top of the stack.
    /// Returns an error if the stack is empty.
    pub fn pop(&mut self) -> Result<f64, CalcError> {
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
    pub fn back(&self) -> Result<f64, CalcError> {
        self.stack.last().copied().ok_or(CalcError::StackUnderflow {
            needed: 1,
            found: 0,
        })
    }
}

/// Executes a sequence of pre-parsed `Operator`s starting with `input_stack`.
///
/// The interpreter uses a simple operand stack of `f64` values. Procedures
/// (blocks for `if` / `ifelse`) are represented as nested `Vec<Operator>` and
/// are executed recursively with a cloned snapshot of the current stack.
///
/// Returned is the final stack contents (bottom-to-top order) on success.
///
/// Errors include stack underflow, division by zero, square root of a negative
/// number, and invalid counts for `roll` / `copy`.
pub fn execute(input_stack: &[f64], ops: &[Operator]) -> Result<Vec<f64>, CalcError> {
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

        let op = &frame.ops[frame.ip];
        // Advance before executing (important for pushing child frames)
        frame.ip = frame
            .ip
            .checked_add(1)
            .ok_or(CalcError::ArithmeticOverflow { op: "ip_inc" })?;
        match op {
            Operator::Add => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(a + b)?;
            }
            Operator::Sub => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(a - b)?;
            }
            Operator::Mul => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(a * b)?;
            }
            Operator::Div => {
                let b = frame.pop()?;
                if b == 0.0 {
                    return Err(CalcError::DivisionByZero);
                }
                let a = frame.pop()?;
                frame.push(a / b)?;
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
                frame.push(if a == b { 1.0 } else { 0.0 })?;
            }
            Operator::Ne => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(if a != b { 1.0 } else { 0.0 })?;
            }
            Operator::Gt => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(if a > b { 1.0 } else { 0.0 })?;
            }
            Operator::Lt => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(if a < b { 1.0 } else { 0.0 })?;
            }
            Operator::Ge => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(if a >= b { 1.0 } else { 0.0 })?;
            }
            Operator::Le => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(if a <= b { 1.0 } else { 0.0 })?;
            }
            Operator::And => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(if a != 0.0 && b != 0.0 { 1.0 } else { 0.0 })?;
            }
            Operator::Or => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                frame.push(if a != 0.0 || b != 0.0 { 1.0 } else { 0.0 })?;
            }
            Operator::Not => {
                let a = frame.pop()?;
                frame.push(if a == 0.0 { 1.0 } else { 0.0 })?;
            }
            Operator::If(block) => {
                let cond = frame.pop()?;
                if cond != 0.0 {
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
                let cond = frame.pop()?;
                let chosen = if cond != 0.0 { block1 } else { block2 };
                let snapshot = frame.stack.clone();
                frames.push(Frame {
                    ops: chosen,
                    ip: 0,
                    stack: snapshot,
                });
            }
            Operator::Copy => {
                let n_val = frame.pop()?;
                let n = n_val.to_usize().ok_or(CalcError::InvalidIntegerOperand {
                    op: "copy",
                    value: n_val,
                })?;

                let len = frame.len();
                let start = len
                    .checked_sub(n)
                    .ok_or(CalcError::ArithmeticOverflow { op: "copy_index" })?;
                let to_copy: Vec<f64> = frame.stack[start..].to_vec();
                for v in to_copy {
                    frame.push(v)?;
                }
            }
            Operator::Sqrt => {
                let a = frame.pop()?;
                if a < 0.0 {
                    return Err(CalcError::NegativeSqrt);
                }
                frame.push(a.sqrt())?;
            }
            Operator::Mod => {
                let b = frame.pop()?;
                if b == 0.0 {
                    return Err(CalcError::DivisionByZero);
                }
                let a = frame.pop()?;
                frame.push(a % b)?;
            }
            Operator::Cvi => {
                let a = frame.pop()?;
                frame.push(a.trunc())?;
            }
            Operator::Abs => {
                let a = frame.pop()?;
                frame.push(a.abs())?;
            }
            Operator::Roll => {
                let m_val = frame.pop()?;
                let n_val = frame.pop()?;
                let m = m_val.to_isize().ok_or(CalcError::NegativeIntegerOperand {
                    op: "roll",
                    value: m_val,
                })?;
                let n = n_val.to_usize().ok_or(CalcError::InvalidIntegerOperand {
                    op: "roll",
                    value: n_val,
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
                    // Rotate only if there's an actual shift.
                    frame.stack[start..].rotate_right(m_norm_usize);
                }
            }
            Operator::Truncate => {
                let a = frame.pop()?;
                frame.push(a.trunc())?;
            }
            Operator::Number(num) => frame.push(*num)?,
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
pub fn evaluate_postscript(input_stack: &[f64], code: &str) -> Result<Vec<f64>, CalcError> {
    let code = code.replace("{", " { ").replace("}", " } ");
    let ops = parse_tokens(&code.split_whitespace().collect::<Vec<_>>())?;
    execute(input_stack, &ops)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

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
        let tokens = vec!["eq", "ne", "gt", "lt", "ge", "le", "and", "or", "not"];
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
                Operator::Not
            ]
        );
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
