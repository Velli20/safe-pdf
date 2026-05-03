//! Decode ranges used to map normalized sample values into output values.

use num_traits::ToPrimitive;

use crate::error::DecodeError;

/// Represents a `/Decode` range for a single PDF sample component.
#[derive(Debug, Clone, Copy)]
pub struct DecodeRange {
    min: f32,
    max: f32,
}

impl DecodeRange {
    /// Creates a new decode range from finite minimum and maximum values.
    pub fn new(min: f32, max: f32) -> Result<Self, DecodeError> {
        if !min.is_finite() || !max.is_finite() {
            return Err(DecodeError::InvalidDecodeValue);
        }

        Ok(Self { min, max })
    }

    /// Returns the identity decode range.
    pub fn identity() -> Self {
        Self { min: 0.0, max: 1.0 }
    }

    /// Returns the inverted identity decode range.
    pub fn inverted_identity() -> Self {
        Self { min: 1.0, max: 0.0 }
    }

    /// Maps one packed sample byte into the requested output range.
    pub fn map_byte(&self, sample: u8, sample_max: u8, output_max: u8) -> u8 {
        let sample_max = f32::from(sample_max.max(1));
        let normalized = f32::from(sample) / sample_max;
        let decoded = self.min + normalized * (self.max - self.min);
        let scaled = decoded * f32::from(output_max);
        let clamped = scaled.clamp(0.0, f32::from(output_max));
        clamped.round().to_u8().unwrap_or(u8::MAX)
    }
}
