use crate::calculator::CalcError;

/// CalculatorStack is a thin wrapper around `Vec<f64>` that provides stack-like operations
/// for use in a PostScript calculator context. It supports push, pop, and back operations
/// and returns CalcError on underflow conditions.
pub(crate) struct CalculatorStack(pub Vec<f64>);

/// Allows creating a CalculatorStack from a slice of f64 values.
impl From<&[f64]> for CalculatorStack {
    fn from(input: &[f64]) -> Self {
        CalculatorStack(input.to_vec())
    }
}

impl CalculatorStack {
    /// Returns the number of elements in the stack.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Pushes a value onto the top of the stack.
    pub fn push(&mut self, value: f64) {
        self.0.push(value);
    }

    /// Pops a value from the top of the stack.
    /// Returns an error if the stack is empty.
    pub fn pop(&mut self) -> Result<f64, CalcError> {
        self.0.pop().ok_or(CalcError::StackUnderflow {
            needed: 1,
            found: 0,
        })
    }

    /// Returns the value at the top of the stack without removing it.
    /// Returns an error if the stack is empty.
    pub fn back(&self) -> Result<f64, CalcError> {
        self.0.last().copied().ok_or(CalcError::StackUnderflow {
            needed: 1,
            found: 0,
        })
    }
}
