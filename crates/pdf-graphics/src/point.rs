/// A 2D point.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "typescript",
    ts(export_to = "annotation_contract.ts", concrete(T = f32))
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

impl Point<f32> {
    /// Whether both coordinates are finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Moves the point by the given displacement.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.x += dx;
        self.y += dy;
    }

    /// Returns the point at fraction `t` of the way from `self` towards `to`.
    pub fn lerp(self, to: Self, t: f32) -> Self {
        Self::new(self.x + (to.x - self.x) * t, self.y + (to.y - self.y) * t)
    }
}
