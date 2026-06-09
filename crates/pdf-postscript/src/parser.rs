use crate::{calculator::CalcError, operator::Operator, value::Value};

/// Represents a block of operations.
struct ProgramBlock {
    ops: Vec<Vec<Operator>>,
}

impl ProgramBlock {
    /// Create a new empty program block.
    fn new() -> Self {
        Self { ops: vec![vec![]] }
    }

    /// Push a new operator onto the current block.
    fn push(&mut self, op: Operator) -> Result<(), CalcError> {
        self.ops
            .last_mut()
            .ok_or(CalcError::EmptyBlockStack)?
            .push(op);
        Ok(())
    }

    /// Pop the current block and return it.
    fn pop(&mut self) -> Option<Vec<Operator>> {
        self.ops.pop()
    }

    /// Push a new block onto the stack.
    fn push_block(&mut self) -> Result<(), CalcError> {
        self.ops.push(vec![]);
        Ok(())
    }
}

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
    let mut block_stack = ProgramBlock::new();
    while i < tokens.len() {
        match tokens
            .get(i)
            .copied()
            .ok_or(CalcError::TokenIndexOverflow)?
        {
            "add" => block_stack.push(Operator::Add)?,
            "sub" => block_stack.push(Operator::Sub)?,
            "mul" => block_stack.push(Operator::Mul)?,
            "div" => block_stack.push(Operator::Div)?,
            "idiv" => block_stack.push(Operator::Idiv)?,
            "dup" => block_stack.push(Operator::Dup)?,
            "exch" => block_stack.push(Operator::Exch)?,
            "pop" => block_stack.push(Operator::Pop)?,
            "eq" => block_stack.push(Operator::Eq)?,
            "ne" => block_stack.push(Operator::Ne)?,
            "gt" => block_stack.push(Operator::Gt)?,
            "lt" => block_stack.push(Operator::Lt)?,
            "ge" => block_stack.push(Operator::Ge)?,
            "le" => block_stack.push(Operator::Le)?,
            "and" => block_stack.push(Operator::And)?,
            "or" => block_stack.push(Operator::Or)?,
            "xor" => block_stack.push(Operator::Xor)?,
            "not" => block_stack.push(Operator::Not)?,
            "bitshift" => block_stack.push(Operator::Bitshift)?,
            "if" => {
                let block1 = block_stack.pop().ok_or(CalcError::MissingIfBlock)?;
                block_stack.push(Operator::If(block1))?;
            }
            "ifelse" => {
                let block1 = block_stack.pop().ok_or(CalcError::MissingIfElseBlocks)?;
                let block2 = block_stack.pop().ok_or(CalcError::MissingIfElseBlocks)?;
                block_stack.push(Operator::IfElse(block2, block1))?;
            }
            "{" => {
                block_stack.push_block()?;
            }
            "}" => {}
            "copy" => block_stack.push(Operator::Copy)?,
            "index" => block_stack.push(Operator::Index)?,
            "roll" => block_stack.push(Operator::Roll)?,
            "sqrt" => block_stack.push(Operator::Sqrt)?,
            "sin" => block_stack.push(Operator::Sin)?,
            "cos" => block_stack.push(Operator::Cos)?,
            "tan" => block_stack.push(Operator::Tan)?,
            "atan" => block_stack.push(Operator::Atan)?,
            "log" => block_stack.push(Operator::Log)?,
            "cvi" => block_stack.push(Operator::Cvi)?,
            "cvr" => block_stack.push(Operator::Cvr)?,
            "mod" => block_stack.push(Operator::Mod)?,
            "floor" => block_stack.push(Operator::Floor)?,
            "truncate" => block_stack.push(Operator::Truncate)?,
            "abs" => block_stack.push(Operator::Abs)?,
            "true" => block_stack.push(Operator::Number(Value::Bool(true)))?,
            "false" => block_stack.push(Operator::Number(Value::Bool(false)))?,
            t => {
                let value = parse_number(t)?;
                block_stack.push(Operator::Number(value))?;
            }
        }
        i = i.checked_add(1).ok_or(CalcError::TokenIndexOverflow)?;
    }

    block_stack.pop().ok_or(CalcError::EmptyBlockStack)
}

fn parse_number(token: &str) -> Result<Value, CalcError> {
    if token.contains('.') || token.contains('e') || token.contains('E') {
        let value = token
            .parse::<f64>()
            .map_err(|_| CalcError::InvalidNumber(token.to_string()))?;
        if value.is_finite() {
            Ok(Value::Real(value))
        } else {
            Err(CalcError::InvalidNumber(token.to_string()))
        }
    } else {
        token
            .parse::<i32>()
            .map(Value::Integer)
            .map_err(|_| CalcError::InvalidNumber(token.to_string()))
    }
}
