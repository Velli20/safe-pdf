use crate::{
    BoundsAccumulator,
    pdf_path::PdfPath,
    point::Point,
    rect::Rect,
    transform::{Transform, TransformError},
};

use num_traits::ToPrimitive;

/// Distance between squiggle vertices along the baseline.
const SQUIGGLE_STEP: f64 = 2.0;
/// Perpendicular squiggle displacement.
const SQUIGGLE_AMPLITUDE: f64 = 1.0;

/// One sequence of straight segments, with closure stored separately from vertices.
///
/// The vertex coordinate type defaults to `f32`.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "typescript",
    ts(export_to = "annotation_contract.ts", concrete(T = f64))
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

    /// Returns a copy with `f` applied to every vertex, keeping the closure flag.
    pub fn map(&self, f: impl FnMut(Point<T>) -> Point<T>) -> Self
    where
        T: Copy,
    {
        Self {
            points: self.points.iter().copied().map(f).collect(),
            closed: self.closed,
        }
    }
}

impl Polyline<f32> {
    /// Moves every vertex by `delta` in the current coordinate space.
    pub fn translate(&mut self, delta: Point) {
        for point in &mut self.points {
            point.x += delta.x;
            point.y += delta.y;
        }
    }

    /// Maps vertices in place, with the same unchecked semantics as `PdfPath::transform`.
    pub fn transform(&mut self, transform: &Transform) {
        for point in &mut self.points {
            let (x, y) = transform.transform_point(point.x, point.y);
            *point = Point::new(x, y);
        }
    }

    /// Returns transformed axis-aligned bounds without allocating.
    ///
    /// Empty geometry returns `None`; single points and straight lines retain their
    /// degenerate rectangles. Nonfinite transforms, inputs, or outputs return an error.
    pub fn bounds(&self, transform: &Transform) -> Result<Option<Rect>, TransformError> {
        transform.validate()?;
        let mut bounds = BoundsAccumulator::new();
        for &point in &self.points {
            bounds.include(transform.try_map_point(point)?);
        }
        Ok(bounds.finish())
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
}

impl Polyline<f64> {
    /// Moves every vertex by the given displacement without validating.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        for point in &mut self.points {
            point.translate(dx, dy);
        }
    }

    /// Axis-aligned rectangle enclosing every vertex; `None` for empty geometry.
    ///
    /// Unlike [`Polyline::<f32>::bounds`], vertices are taken as-is: no transform is
    /// applied and nonfinite coordinates propagate into the result unvalidated.
    pub fn bounds(&self) -> Option<Rect<f64>> {
        self.points
            .iter()
            .map(|point| Rect {
                left: point.x,
                top: point.y,
                right: point.x,
                bottom: point.y,
            })
            .reduce(|bounds, point| bounds.union(&point))
    }

    /// Creates an open zigzag around the baseline from `a` to `b`.
    ///
    /// Baseline intervals are evenly divided into steps of at most 2.0 coordinate
    /// units, with alternating perpendicular offsets of -1.0 and +1.0 units.
    /// The endpoints are baseline anchors, not the first and last generated vertices.
    /// Coincident anchors produce two coincident vertices. Inputs are not validated.
    pub fn squiggle(a: Point<f64>, b: Point<f64>) -> Self {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length = dx.hypot(dy);
        let unit = if length > 0.0 { length } else { 1.0 };
        let steps = (length / SQUIGGLE_STEP).ceil().max(1.0);
        let count = steps.to_usize().unwrap_or(1);
        let points = (0..=count)
            .map(|i| {
                let t = i.to_f64().unwrap_or(0.0) / steps;
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
            assert_eq!(line.bounds(&Transform::identity()).unwrap(), None);
        }
        let line = Polyline::<f32>::new(vec![Point::new(-2.0, 3.0)], true);
        assert_eq!(
            line.bounds(&Transform::identity()).unwrap(),
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
        assert_eq!(line.bounds(&transform).unwrap(), expected);
        line.transform(&transform);
        assert_eq!(line.points(), &[Point::new(2.0, 2.0), Point::new(2.0, 0.0)]);
        assert!(line.is_closed());
        assert_eq!(line.bounds(&Transform::identity()).unwrap(), expected);
    }

    #[test]
    fn bounds_reject_invalid_coordinates_and_overflow() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let line = Polyline::new(vec![Point::new(0.0, 0.0), Point::new(invalid, 1.0)], false);
            assert_eq!(
                line.bounds(&Transform::identity()),
                Err(TransformError::NonFinite)
            );
            assert_eq!(
                Polyline::<f32>::default().bounds(&Transform::from_translate(0.0, invalid)),
                Err(TransformError::NonFinite)
            );
        }
        let line = Polyline::new(vec![Point::new(f32::MAX, 0.0)], false);
        assert_eq!(
            line.bounds(&Transform::from_scale(2.0, 1.0)),
            Err(TransformError::NonFinite)
        );
    }
}
