use crate::{
    BoundsAccumulator,
    geometry::{Bounds, Translate},
    point::Point,
    polyline::Polyline,
    rect::Rect,
};

/// Quadrilateral with four corners in perimeter order.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "typescript",
    ts(export_to = "annotation_contract.ts", concrete(T = f32))
)]
pub struct Quad<T = f32> {
    /// Clockwise or counterclockwise perimeter; crossing edges are invalid.
    pub corners: [Point<T>; 4],
}

impl<T: Copy> From<&Quad<T>> for Polyline<T> {
    /// The four corners as a closed outline, in perimeter order.
    fn from(quad: &Quad<T>) -> Self {
        Polyline::new(quad.corners.to_vec(), true)
    }
}

impl Bounds for Quad<f32> {
    /// Rectangle enclosing all four corners, with `left`/`top` the minima.
    fn bounds(&self) -> Option<Rect> {
        if !self.is_finite() {
            return None;
        }
        let mut bounds = BoundsAccumulator::new();
        for &corner in &self.corners {
            bounds.include(corner);
        }
        bounds.finish()
    }
}

impl Translate for Quad<f32> {
    fn translate(&mut self, delta: Point) {
        for corner in &mut self.corners {
            corner.translate(delta);
        }
    }
}

impl Quad<f32> {
    /// Whether every corner is finite.
    pub fn is_finite(&self) -> bool {
        self.corners.iter().all(|corner| corner.is_finite())
    }

    /// Whether the perimeter turns consistently in one direction, so the
    /// quadrilateral is strictly convex without crossing edges.
    pub fn is_convex(&self) -> bool {
        let [a, b, c, d] = self.corners;
        let turns = [turn(a, b, c), turn(b, c, d), turn(c, d, a), turn(d, a, b)];
        turns.iter().all(|value| *value > 0.0) || turns.iter().all(|value| *value < 0.0)
    }
}

// Positive scaling of each axis preserves winding. Normalize edge differences
// independently so huge offsets, subnormal extents, and extreme aspect ratios do
// not overflow or underflow merely when computing a cross product.
fn turn(a: Point, b: Point, c: Point) -> f32 {
    let normalize = |a: f32, b: f32, c: f32| {
        let (mut ab, mut bc) = (b - a, c - b);
        if !ab.is_finite() || !bc.is_finite() {
            ab = b * 0.5 - a * 0.5;
            bc = c * 0.5 - b * 0.5;
        }
        let scale = ab.abs().max(bc.abs());
        if scale == 0.0 {
            (0.0, 0.0)
        } else {
            (ab / scale, bc / scale)
        }
    };
    let (ab_x, bc_x) = normalize(a.x, b.x, c.x);
    let (ab_y, bc_y) = normalize(a.y, b.y, c.y);
    ab_x * bc_y - ab_y * bc_x
}
