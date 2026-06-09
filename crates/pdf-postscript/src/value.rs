use crate::calculator::CalcError;

/// A typed operand value used by the PostScript calculator.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Value {
    /// A 32-bit signed PostScript integer.
    Integer(i32),
    /// A finite floating-point real value.
    Real(f64),
    /// A PostScript boolean.
    Bool(bool),
}

impl Value {
    /// Converts a numeric value to `f64`.
    pub fn to_f64(self, op: &'static str) -> Result<f64, CalcError> {
        match self {
            Self::Integer(value) => Ok(f64::from(value)),
            Self::Real(value) => Ok(value),
            Self::Bool(_) => Err(CalcError::InvalidOperandType {
                op,
                expected: "number",
                found: self.type_name(),
            }),
        }
    }

    /// Converts a value to an integer, requiring it to already have integer type.
    pub fn to_i32(self, op: &'static str) -> Result<i32, CalcError> {
        match self {
            Self::Integer(value) => Ok(value),
            Self::Real(value) => Err(CalcError::InvalidIntegerOperand { op, value }),
            Self::Bool(_) => Err(CalcError::InvalidOperandType {
                op,
                expected: "integer",
                found: self.type_name(),
            }),
        }
    }

    /// Converts a value to a boolean, requiring it to already have boolean type.
    pub fn to_bool(self, op: &'static str) -> Result<bool, CalcError> {
        match self {
            Self::Bool(value) => Ok(value),
            Self::Integer(_) | Self::Real(_) => Err(CalcError::InvalidOperandType {
                op,
                expected: "bool",
                found: self.type_name(),
            }),
        }
    }

    /// Returns the value type name for diagnostics.
    pub fn type_name(self) -> &'static str {
        match self {
            Self::Integer(_) => "integer",
            Self::Real(_) => "real",
            Self::Bool(_) => "bool",
        }
    }
}
