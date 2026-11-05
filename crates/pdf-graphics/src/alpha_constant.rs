//! The PDF alpha constant (`CA` and `ca`).

/// A constant opacity from a `CA` or `ca` entry, always finite and within `[0.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlphaConstant(f32);

impl AlphaConstant {
    /// Full opacity, the value of an absent entry.
    pub const OPAQUE: Self = Self(1.0);

    /// Returns the alpha constant for `value`, clamped to `[0.0, 1.0]`.
    ///
    /// A nonfinite value is opaque.
    pub const fn new(value: f32) -> Self {
        if value.is_finite() {
            Self(value.clamp(0.0, 1.0))
        } else {
            Self::OPAQUE
        }
    }

    /// Returns the opacity, from 0.0 to 1.0.
    pub const fn value(self) -> f32 {
        self.0
    }
}

impl Default for AlphaConstant {
    fn default() -> Self {
        Self::OPAQUE
    }
}
