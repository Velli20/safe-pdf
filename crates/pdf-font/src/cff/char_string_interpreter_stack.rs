use num_traits::FromPrimitive;
use thiserror::Error;

/// Error variants that may occur while evaluating a Type 2 CharString operator.
#[derive(Debug, Error)]
pub enum CharStringStackError {
    #[error("Stack underflow")]
    StackUnderflow,
    #[error("Invalid operand count: expected {expected}, found {found}")]
    InvalidOperandCount { expected: usize, found: usize },
    #[error("Numeric conversion error")]
    NumericConversionError,
}

#[derive(Default)]
pub struct CharStringStack {
    pub operands: Vec<f32>,
    pub is_open: bool,
    pub have_read_width: bool,
    pub x: f32,
    pub y: f32,
    pub stack_index: usize,
}

impl CharStringStack {
    pub fn push(&mut self, v: i32) -> Result<(), CharStringStackError> {
        self.operands
            .push(f32::from_i32(v).ok_or(CharStringStackError::NumericConversionError)?);
        Ok(())
    }

    pub fn pop(&mut self) -> Option<f32> {
        self.operands.pop()
    }

    pub fn len(&self) -> usize {
        self.operands.len()
    }

    pub fn clear(&mut self) {
        self.operands.clear();
        self.stack_index = 0;
    }

    pub fn get_fixed(&self, index: usize) -> Result<f32, CharStringStackError> {
        self.operands
            .get(index)
            .copied()
            .ok_or(CharStringStackError::StackUnderflow)
    }

    pub fn coords_remaining(&self) -> usize {
        self.operands.len().saturating_sub(self.stack_index)
    }

    pub fn fixed_array<const N: usize>(
        &self,
        first_index: usize,
    ) -> Result<[f32; N], CharStringStackError> {
        let end = first_index
            .checked_add(N)
            .ok_or(CharStringStackError::StackUnderflow)?;

        let slice = self
            .operands
            .get(first_index..end)
            .ok_or(CharStringStackError::StackUnderflow)?;

        let mut arr = [0.0; N];
        arr.copy_from_slice(slice);
        Ok(arr)
    }

    pub fn len_is_odd(&self) -> bool {
        self.operands.len() % 2 == 1
    }
}
