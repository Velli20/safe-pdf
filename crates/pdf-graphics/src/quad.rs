use crate::{point::Point, polyline::Polyline, rect::Rect};

/// Quadrilateral with four corners in perimeter order.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "typescript",
    ts(export_to = "annotation_contract.ts", concrete(T = f64))
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

impl Quad<f64> {
    /// Whether every corner is finite.
    pub fn is_finite(&self) -> bool {
        self.corners.iter().all(|corner| corner.is_finite())
    }

    /// Axis-aligned rectangle enclosing all four corners, with `left`/`top` the minima.
    pub fn bounds(&self) -> Rect<f64> {
        let [first, rest @ ..] = self.corners;
        rest.iter().fold(
            Rect {
                left: first.x,
                top: first.y,
                right: first.x,
                bottom: first.y,
            },
            |bounds, corner| {
                bounds.union(&Rect {
                    left: corner.x,
                    top: corner.y,
                    right: corner.x,
                    bottom: corner.y,
                })
            },
        )
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
fn turn(a: Point<f64>, b: Point<f64>, c: Point<f64>) -> f64 {
    let normalize = |a: f64, b: f64, c: f64| {
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
