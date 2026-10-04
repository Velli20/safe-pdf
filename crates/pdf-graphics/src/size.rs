/// A generic two-dimensional size value with a `f32` default coordinate type.
///
/// The type stores the width and height as a width/height pair and is intended
/// as the reusable size analogue for the crate's other geometry models.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Size<T = f32> {
    /// The horizontal dimension of the size.
    pub width: T,
    /// The vertical dimension of the size.
    pub height: T,
}

impl<T> Size<T> {
    /// Creates a new size from the supplied width and height.
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }

    /// Swaps the width and height when `degrees` is a sideways quarter turn (90 or 270
    /// after normalizing into `0..360`).
    pub fn quarter_turned(self, degrees: i32) -> Self {
        match degrees.rem_euclid(360) {
            90 | 270 => Self::new(self.height, self.width),
            _ => self,
        }
    }
}

impl Size<f32> {
    /// Returns `true` when the width and height are finite and strictly positive.
    pub fn validate(&self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width > 0.0 && self.height > 0.0
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn size_f32_validate_reports_finite_positive_dimensions() {
        use crate::size::Size;

        let size = Size::new(10.0, 20.0);
        assert!(size.validate());

        let invalid = Size {
            width: f32::INFINITY,
            height: 20.0,
        };
        assert!(!invalid.validate());
    }
}
