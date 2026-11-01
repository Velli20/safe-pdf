//! Shape-independent bounds and translation, implemented by each shape type.

use crate::{point::Point, rect::Rect};

/// Shapes with an axis-aligned bounding rectangle.
pub trait Bounds {
    /// Enclosing rectangle; `None` for empty geometry or any nonfinite coordinate.
    fn bounds(&self) -> Option<Rect>;
}

/// Shapes that move rigidly by a displacement.
pub trait Translate {
    /// Moves every coordinate by `delta` without validating the result.
    fn translate(&mut self, delta: Point);
}

impl<T: Bounds> Bounds for [T] {
    /// Union of every element's bounds; `None` when empty or any element has none.
    fn bounds(&self) -> Option<Rect> {
        self.iter()
            .map(T::bounds)
            .reduce(|a, b| Some(a?.union(&b?)))
            .flatten()
    }
}

impl<T: Translate> Translate for Vec<T> {
    fn translate(&mut self, delta: Point) {
        for item in self {
            item.translate(delta);
        }
    }
}

impl<T: Translate> Translate for Option<T> {
    fn translate(&mut self, delta: Point) {
        if let Some(item) = self {
            item.translate(delta);
        }
    }
}
