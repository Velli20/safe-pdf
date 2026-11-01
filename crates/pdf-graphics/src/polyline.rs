use crate::{
    BoundsAccumulator,
    geometry::{Bounds, Translate},
    pdf_path::PdfPath,
    point::Point,
    rect::Rect,
    transform::Transform,
};

use num_traits::ToPrimitive;

/// Distance between squiggle vertices along the baseline.
const SQUIGGLE_STEP: f32 = 2.0;
/// Perpendicular squiggle displacement.
const SQUIGGLE_AMPLITUDE: f32 = 1.0;

/// One sequence of straight segments, with closure stored separately from vertices.
///
/// The vertex coordinate type defaults to `f32`.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "typescript",
    ts(export_to = "annotation_contract.ts", concrete(T = f32))
)]
pub struct Polyline<T = f32> {
    /// Vertices in order, without duplication of the closing point.
    pub points: Vec<Point<T>>,
    /// Whether the last vertex connects to the first.
    pub closed: bool,
}

impl<T> Polyline<T> {
    /// Takes ownership of vertices without copying or duplicating the closing point.
    pub fn new(points: Vec<Point<T>>, closed: bool) -> Self {
        Self { points, closed }
    }

    /// Borrows the stored vertices in their original order.
    pub fn points(&self) -> &[Point<T>] {
        &self.points
    }

    /// Returns whether the last vertex connects to the first.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Connects the last vertex to the first without adding a vertex.
    pub fn close(&mut self) {
        self.closed = true;
    }
}

impl<T: Copy> From<&Rect<T>> for Polyline<T> {
    /// The four corners as a closed outline: left-top, right-top, right-bottom, left-bottom.
    fn from(rect: &Rect<T>) -> Self {
        Self::new(
            vec![
                Point::new(rect.left, rect.top),
                Point::new(rect.right, rect.top),
                Point::new(rect.right, rect.bottom),
                Point::new(rect.left, rect.bottom),
            ],
            true,
        )
    }
}

impl Bounds for Polyline<f32> {
    /// Single points and straight lines keep their degenerate rectangles.
    fn bounds(&self) -> Option<Rect> {
        let mut bounds = BoundsAccumulator::new();
        for &point in &self.points {
            if !point.is_finite() {
                return None;
            }
            bounds.include(point);
        }
        bounds.finish()
    }
}

impl Translate for Polyline<f32> {
    fn translate(&mut self, delta: Point) {
        for point in &mut self.points {
            point.translate(delta);
        }
    }
}

impl Polyline<f32> {
    /// Maps vertices in place, with the same unchecked semantics as `PdfPath::transform`.
    pub fn transform(&mut self, transform: &Transform) {
        for point in &mut self.points {
            let (x, y) = transform.transform_point(point.x, point.y);
            *point = Point::new(x, y);
        }
    }

    /// Materializes path verbs, adding a closing verb only for nonempty closed geometry.
    pub fn to_pdf_path(&self) -> PdfPath {
        let mut path = PdfPath::default();
        if let Some((first, remaining)) = self.points.split_first() {
            path.move_to(first.x, first.y);
            for point in remaining {
                path.line_to(point.x, point.y);
            }
            if self.closed {
                path.close();
            }
        }
        path
    }

    /// Creates an open zigzag around the baseline from `a` to `b`.
    ///
    /// Baseline intervals are evenly divided into steps of at most 2.0 coordinate
    /// units, with alternating perpendicular offsets of -1.0 and +1.0 units.
    /// The endpoints are baseline anchors, not the first and last generated vertices.
    /// Coincident anchors produce two coincident vertices. Inputs are not validated.
    pub fn squiggle(a: Point, b: Point) -> Self {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length = dx.hypot(dy);
        let unit = if length > 0.0 { length } else { 1.0 };
        let steps = (length / SQUIGGLE_STEP).ceil().max(1.0);
        let count = steps.to_usize().unwrap_or(1);
        let points = (0..=count)
            .map(|i| {
                let t = i.to_f32().unwrap_or(0.0) / steps;
                let shift = if i % 2 == 1 {
                    SQUIGGLE_AMPLITUDE
                } else {
                    -SQUIGGLE_AMPLITUDE
                };
                Point::new(
                    a.x + dx * t - dy / unit * shift,
                    a.y + dy * t + dx / unit * shift,
                )
            })
            .collect();
        Self::new(points, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_path::PathVerb;

    #[test]
    fn closure_is_separate_and_conversion_preserves_vertices() {
        let points = vec![Point::new(1.0, 2.0), Point::new(3.0, 4.0)];
        let allocation = points.as_ptr();
        let mut line = Polyline::new(points, false);
        assert_eq!(line.points().as_ptr(), allocation);
        assert_eq!(
            line.to_pdf_path().verbs,
            vec![
                PathVerb::MoveTo { x: 1.0, y: 2.0 },
                PathVerb::LineTo { x: 3.0, y: 4.0 },
            ]
        );
        line.close();
        assert!(line.is_closed());
        assert_eq!(line.points().len(), 2);
        assert_eq!(line.to_pdf_path().verbs.last(), Some(&PathVerb::Close));
        assert_eq!(
            line.to_pdf_path()
                .try_polyline_points(&Transform::identity())
                .unwrap(),
            vec![
                Point::new(1.0, 2.0),
                Point::new(3.0, 4.0),
                Point::new(1.0, 2.0)
            ]
        );
    }

    #[test]
    fn empty_and_single_point_geometry() {
        for closed in [false, true] {
            let line = Polyline::new(Vec::new(), closed);
            assert!(line.to_pdf_path().verbs.is_empty());
            assert_eq!(line.bounds(), None);
        }
        let line = Polyline::<f32>::new(vec![Point::new(-2.0, 3.0)], true);
        assert_eq!(
            line.bounds(),
            Some(Rect {
                left: -2.0,
                top: 3.0,
                right: -2.0,
                bottom: 3.0,
            })
        );
    }

    #[test]
    fn bounds_map_vertices_before_accumulating() {
        let mut line = Polyline::<f32>::new(vec![Point::new(0.0, 2.0), Point::new(2.0, 0.0)], true);
        // Shearing these vertices gives a vertical line, unlike mapping their source AABB.
        let transform = Transform::from_row(1.0, 0.0, 1.0, 1.0, 0.0, 0.0);
        let expected = Some(Rect {
            left: 2.0,
            top: 0.0,
            right: 2.0,
            bottom: 2.0,
        });
        line.transform(&transform);
        assert_eq!(line.points(), &[Point::new(2.0, 2.0), Point::new(2.0, 0.0)]);
        assert!(line.is_closed());
        assert_eq!(line.bounds(), expected);
    }

    #[test]
    fn bounds_reject_invalid_coordinates() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let line = Polyline::new(vec![Point::new(0.0, 0.0), Point::new(invalid, 1.0)], false);
            assert_eq!(line.bounds(), None);
        }
    }
}
