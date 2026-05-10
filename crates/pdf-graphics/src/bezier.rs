use crate::point::Point;

/// Evaluates a cubic Bézier curve at parameter `t`.
///
/// The four input points are the start point, two control points, and the end point.
/// The parameter `t` is expected to be in the range `[0.0, 1.0]`.
pub fn evaluate_cubic_bezier(points: [Point; 4], t: f32) -> Point {
    let basis = bernstein_basis(t);
    let (x, y) = points
        .into_iter()
        .zip(basis)
        .fold((0.0, 0.0), |(x, y), (point, weight)| {
            (x + point.x * weight, y + point.y * weight)
        });

    Point::new(x, y)
}

/// Computes the cubic Bernstein basis weights for parameter `t`.
///
/// The returned weights correspond to the four control points of a cubic Bézier curve.
pub fn bernstein_basis(t: f32) -> [f32; 4] {
    let one_minus_t = 1.0 - t;
    [
        one_minus_t * one_minus_t * one_minus_t,
        3.0 * t * one_minus_t * one_minus_t,
        3.0 * t * t * one_minus_t,
        t * t * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::{bernstein_basis, evaluate_cubic_bezier};
    use crate::point::Point;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn evaluate_cubic_bezier_returns_start_and_end_points() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 2.0),
            Point::new(2.0, 3.0),
            Point::new(4.0, 5.0),
        ];

        assert_eq!(evaluate_cubic_bezier(points, 0.0), points[0]);
        assert_eq!(evaluate_cubic_bezier(points, 1.0), points[3]);
    }

    #[test]
    fn evaluate_cubic_bezier_matches_known_midpoint() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(1.0, 1.0),
            Point::new(1.0, 0.0),
        ];

        let point = evaluate_cubic_bezier(points, 0.5);

        assert!(approx_eq(point.x, 0.5));
        assert!(approx_eq(point.y, 0.75));
    }

    #[test]
    fn bernstein_basis_weights_sum_to_one() {
        let basis = bernstein_basis(0.37);

        assert!(approx_eq(basis[0] + basis[1] + basis[2] + basis[3], 1.0));
    }

    #[test]
    fn bernstein_basis_matches_endpoint_behavior() {
        assert_eq!(bernstein_basis(0.0), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(bernstein_basis(1.0), [0.0, 0.0, 0.0, 1.0]);
    }
}
