use crate::{calculator::CalcError, operator::Operator};

/// Parse a linear slice of token strings into a flat `Vec<Operator>`.
///
/// The grammar handled is intentionally small: numeric literals, the subset of
/// operators defined in [`Operator`], and procedure blocks delimited by `{` and
/// `}` which are collected and stored inside `Operator::If` / `Operator::IfElse`.
///
/// Block parsing uses a stack (`block_stack`) so nesting is supported (only as
/// needed by tests for `if` / `ifelse`). A closing brace currently acts as a
/// marker; actual attachment to control operators happens when encountering
/// `if` / `ifelse`.
pub fn parse_tokens(tokens: &[&str]) -> Result<Vec<Operator>, CalcError> {
    let mut i = 0;
    let mut block_stack: Vec<Vec<Operator>> = vec![vec![]];
    while i < tokens.len() {
        match tokens[i] {
            "add" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Add),
            "sub" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Sub),
            "mul" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Mul),
            "div" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Div),
            "dup" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Dup),
            "exch" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Exch),
            "pop" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Pop),
            "eq" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Eq),
            "ne" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Ne),
            "gt" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Gt),
            "lt" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Lt),
            "ge" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Ge),
            "le" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Le),
            "and" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::And),
            "or" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Or),
            "not" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Not),
            "if" => {
                let block1 = block_stack.pop().ok_or(CalcError::EmptyBlockStack)?;
                block_stack
                    .last_mut()
                    .ok_or(CalcError::EmptyBlockStack)?
                    .push(Operator::If(block1));
            }
            "ifelse" => {
                let block1 = block_stack.pop().ok_or(CalcError::EmptyBlockStack)?;
                let block2 = block_stack.pop().ok_or(CalcError::EmptyBlockStack)?;
                block_stack
                    .last_mut()
                    .ok_or(CalcError::EmptyBlockStack)?
                    .push(Operator::IfElse(block2, block1));
            }
            "{" => {
                block_stack.push(vec![]);
            }
            "}" => {}
            "copy" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Copy),
            "roll" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Roll),
            "sqrt" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Sqrt),
            "cvi" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Cvi),
            "mod" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Mod),
            "truncate" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Truncate),
            "abs" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Abs),
            "true" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Number(1.0)),
            "false" => block_stack
                .last_mut()
                .ok_or(CalcError::EmptyBlockStack)?
                .push(Operator::Number(0.0)),
            t => {
                let num = t
                    .parse::<f64>()
                    .map_err(|_| CalcError::InvalidNumber(t.to_string()))?;
                block_stack
                    .last_mut()
                    .ok_or(CalcError::EmptyBlockStack)?
                    .push(Operator::Number(num));
            }
        }
        i += 1;
    }

    block_stack.pop().ok_or(CalcError::EmptyBlockStack)
}
