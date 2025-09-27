use thiserror::Error;

/// Error variants that may occur while evaluating a Type 2 CharString operator.
#[derive(Debug, Error)]
pub enum CharStringStackError {
    #[error("Stack underflow")]
    StackUnderflow,
    #[error("Invalid operand count: expected {expected}, found {found}")]
    InvalidOperandCount { expected: usize, found: usize },
}

#[derive(Default)]
pub struct CharStringStack {
    pub operands: Vec<i32>,
    pub is_open: bool,
    pub have_read_width: bool,
    pub x: f32,
    pub y: f32,
    pub stack_ix: usize,
}

impl CharStringStack {
    pub fn push(&mut self, v: i32) {
        self.operands.push(v);
    }

    pub fn pop(&mut self) -> Option<i32> {
        self.operands.pop()
    }

    pub fn len(&self) -> usize {
        self.operands.len()
    }

    pub fn clear(&mut self) {
        self.operands.clear();
        self.stack_ix = 0;
    }

    pub fn get_fixed(&self, index: usize) -> Result<f32, CharStringStackError> {
        if index >= self.operands.len() {
            return Err(CharStringStackError::StackUnderflow);
        }
        Ok(self.operands[index] as f32)
    }

    pub fn coords_remaining(&self) -> usize {
        // This is overly defensive to avoid overflow but in the case of
        // broken fonts, just return 0 when stack_ix > stack_len to prevent
        // potential runaway while loops in the evaluator if this wraps
        self.operands.len().saturating_sub(self.stack_ix)
    }

    pub fn fixed_array<const N: usize>(
        &self,
        first_index: usize,
    ) -> Result<[f32; N], CharStringStackError> {
        if first_index + N > self.operands.len() {
            return Err(CharStringStackError::StackUnderflow);
        }
        let mut arr = [0.0; N];
        for i in 0..N {
            arr[i] = self.operands[first_index + i] as f32;
        }
        Ok(arr)
    }

    /// Returns true if the number of elements on the stack is odd.
    ///
    /// Used for processing some charstring operators where an odd
    /// count represents the presence of the glyph advance width at the
    /// bottom of the stack.
    pub fn len_is_odd(&self) -> bool {
        self.operands.len() % 2 == 1
    }
}
