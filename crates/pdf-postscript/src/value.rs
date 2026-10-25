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

/// Parses a literal word of a Type 4 program: `true`, `false`, a radix integer
/// (`base#digits`), a decimal integer, or a real. An integer outside the 32-bit
/// range reads as a real, as in PostScript.
impl TryFrom<&[u8]> for Value {
    type Error = CalcError;

    fn try_from(word: &[u8]) -> Result<Self, Self::Error> {
        let invalid = || CalcError::InvalidNumber(String::from_utf8_lossy(word).into_owned());
        let text = core::str::from_utf8(word).map_err(|_| invalid())?;
        match text {
            "true" => return Ok(Self::Bool(true)),
            "false" => return Ok(Self::Bool(false)),
            _ => {}
        }
        if let Some((base, digits)) = text.split_once('#') {
            let base = base
                .parse::<u32>()
                .ok()
                .filter(|base| (2..=36).contains(base))
                .ok_or_else(invalid)?;
            // The digits are an unsigned 32-bit pattern, so `16#FFFFFFFF` reads as -1.
            return u32::from_str_radix(digits, base)
                .ok()
                .filter(|_| !digits.starts_with(['+', '-']))
                .map(|value| Self::Integer(value.cast_signed()))
                .ok_or_else(invalid);
        }
        if let Ok(value) = text.parse::<i32>() {
            return Ok(Self::Integer(value));
        }
        text.parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(Self::Real)
            .ok_or_else(invalid)
    }
}
