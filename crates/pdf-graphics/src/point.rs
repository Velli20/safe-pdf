use num_traits::ToPrimitive;

/// A 2D point.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "typescript",
    ts(export_to = "annotation_contract.ts", concrete(T = f64))
)]
pub struct Point<T = f32> {
    /// Horizontal coordinate.
    pub x: T,
    /// Vertical coordinate.
    pub y: T,
}

impl<T> Point<T> {
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl Point<f64> {
    /// Whether both coordinates are finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Moves the point by the given displacement.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    /// Narrows both coordinates to `f32`, or returns `None` unless both stay finite.
    pub fn to_f32(self) -> Option<Point> {
        let narrow = |value: f64| value.to_f32().filter(|value| value.is_finite());
        Some(Point::new(narrow(self.x)?, narrow(self.y)?))
    }
}

impl From<Point> for Point<f64> {
    /// Widens both coordinates losslessly.
    fn from(point: Point) -> Self {
        Self::new(f64::from(point.x), f64::from(point.y))
    }
}
