use crate::error::PdfOperatorError;
use num_traits::FromPrimitive;
use pdf_object_reader::{
    object_error::ObjectError, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
};

/// Reusable storage for the operands belonging to one PDF operator.
///
/// Values are consumed through a cursor instead of being removed from the front
/// of the backing vector. This keeps reads constant-time and lets the content
/// stream parser reuse the allocation for subsequent operators.
pub struct Operands {
    values: Vec<ObjectVariant>,
    position: usize,
}

impl Operands {
    /// Creates an empty operand buffer with the requested capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
            position: 0,
        }
    }

    /// Appends an operand to the buffer.
    pub fn push(&mut self, value: ObjectVariant) {
        self.values.push(value);
    }

    /// Returns the number of operands that have not yet been consumed.
    pub fn len(&self) -> usize {
        self.values.len().saturating_sub(self.position)
    }

    /// Returns whether every operand has been consumed.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the current backing allocation capacity.
    pub fn capacity(&self) -> usize {
        self.values.capacity()
    }

    /// Clears all operands and resets the read cursor while retaining capacity.
    pub fn clear(&mut self) {
        self.values.clear();
        self.position = 0;
    }

    /// Peeks at the next operand without consuming it.
    pub fn peek_next(&self) -> Option<&ObjectVariant> {
        self.values.get(self.position)
    }

    /// Removes and returns the next operand in constant time.
    pub(crate) fn take_next(&mut self) -> Result<ObjectVariant, PdfOperatorError> {
        let Some(value) = self.values.get_mut(self.position) else {
            return Err(PdfOperatorError::OperandMissing {
                expected: "an operand",
            });
        };

        self.position = self.position.saturating_add(1);
        Ok(std::mem::replace(value, ObjectVariant::Null))
    }

    /// Converts and consumes the next `N` numeric operands.
    pub(crate) fn try_array_of<T, const N: usize>(&mut self) -> Result<[T; N], ObjectError>
    where
        T: FromPrimitive + Copy + Default,
    {
        if self.len() < N {
            return Err(ObjectError::InvalidArrayLength {
                expected: N,
                found: self.len(),
            });
        }

        let mut result = [T::default(); N];
        for output in &mut result {
            *output = self
                .take_next()
                .map_err(|_| ObjectError::InvalidArrayLength {
                    expected: N,
                    found: self.len(),
                })?
                .try_number(&PassthroughResolver)?;
        }
        Ok(result)
    }

    /// Reads the next operand as an `f32`.
    pub fn get_f32(&mut self) -> Result<f32, PdfOperatorError> {
        Ok(self.take_next()?.try_number::<f32>(&PassthroughResolver)?)
    }

    /// Reads the next PDF string operand as owned bytes.
    pub fn get_string_bytes(&mut self) -> Result<Vec<u8>, PdfOperatorError> {
        let object = self.take_next()?;
        match object {
            ObjectVariant::HexString(value) | ObjectVariant::LiteralString(value) => Ok(value),
            other => Err(PdfOperatorError::OperandTypeMismatch {
                expected: "a PDF string operand (HexString or LiteralString)",
                found: other.name(),
            }),
        }
    }

    /// Reads the next PDF Name operand as owned bytes.
    pub fn get_name_bytes(&mut self) -> Result<Vec<u8>, PdfOperatorError> {
        let object = self.take_next()?;
        match object {
            ObjectVariant::Name(value) => Ok(value),
            other => Err(PdfOperatorError::OperandTypeMismatch {
                expected: "a Name operand",
                found: other.name(),
            }),
        }
    }

    /// Reads the next operand as a `u8`.
    pub fn get_u8(&mut self) -> Result<u8, PdfOperatorError> {
        Ok(self.take_next()?.try_number::<u8>(&PassthroughResolver)?)
    }

    /// Reads the next array operand as numeric `f32` values.
    pub fn get_f32_array(&mut self) -> Result<Vec<f32>, PdfOperatorError> {
        let array = self.take_next()?;
        Ok(array.try_vec_of::<f32>(&PassthroughResolver)?)
    }
}

impl From<Vec<ObjectVariant>> for Operands {
    fn from(values: Vec<ObjectVariant>) -> Self {
        Self {
            values,
            position: 0,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn operands_are_consumed_in_order_without_reducing_capacity() {
        let values = vec![
            ObjectVariant::Integer(1),
            ObjectVariant::Real(2.5),
            ObjectVariant::Integer(3),
        ];
        let original_capacity = values.capacity();
        let mut operands = Operands::from(values);

        assert_eq!(operands.get_f32().expect("first number should parse"), 1.0);
        assert_eq!(operands.peek_next(), Some(&ObjectVariant::Real(2.5)));
        assert_eq!(
            operands
                .try_array_of::<f32, 2>()
                .expect("remaining numbers should parse"),
            [2.5, 3.0]
        );
        assert!(operands.is_empty());
        assert_eq!(operands.capacity(), original_capacity);
    }

    #[test]
    fn clear_resets_the_cursor_and_reuses_allocation() {
        let mut operands = Operands::with_capacity(6);
        operands.push(ObjectVariant::Integer(7));
        let capacity = operands.capacity();

        assert_eq!(operands.get_u8().expect("number should parse"), 7);
        operands.clear();
        operands.push(ObjectVariant::Integer(8));

        assert_eq!(operands.get_u8().expect("reused number should parse"), 8);
        assert!(operands.capacity() >= capacity);
    }

    #[test]
    fn grouped_conversion_consumes_through_the_invalid_operand() {
        let mut operands = Operands::from(vec![
            ObjectVariant::Integer(1),
            ObjectVariant::LiteralString(b"not a number".to_vec()),
            ObjectVariant::Integer(3),
        ]);

        let result = operands.try_array_of::<f32, 2>();

        assert!(matches!(
            result,
            Err(ObjectError::TypeMismatch("Number", _))
        ));
        assert_eq!(operands.peek_next(), Some(&ObjectVariant::Integer(3)));
    }

    #[test]
    fn grouped_conversion_rejects_too_few_operands_without_consuming() {
        let mut operands = Operands::from(vec![ObjectVariant::Integer(1)]);

        assert!(matches!(
            operands.try_array_of::<f32, 2>(),
            Err(ObjectError::InvalidArrayLength {
                expected: 2,
                found: 1
            })
        ));
        assert_eq!(operands.len(), 1);
    }

    #[test]
    fn name_operand_preserves_non_utf8_bytes() {
        let mut operands = Operands::from(vec![ObjectVariant::Name(vec![0xFF])]);

        assert_eq!(
            operands
                .get_name_bytes()
                .expect("PDF Names may contain arbitrary bytes"),
            [0xFF]
        );
    }
}
