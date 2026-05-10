use crate::{point::Point, rect::Rect};

/// Accumulates the axis-aligned bounds of a set of points.
///
/// Create a new accumulator with [`BoundsAccumulator::new`], add points with
/// [`BoundsAccumulator::include`], and call [`BoundsAccumulator::finish`] to
/// obtain the final bounding rectangle.
#[derive(Debug, Clone, Copy)]
pub struct BoundsAccumulator {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl BoundsAccumulator {
    /// Creates an empty bounds accumulator.
    pub fn new() -> Self {
        Self {
            min_x: f32::INFINITY,
            min_y: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            max_y: f32::NEG_INFINITY,
        }
    }

    /// Expands the accumulated bounds to include `point`.
    pub fn include(&mut self, point: Point) {
        self.min_x = self.min_x.min(point.x);
        self.min_y = self.min_y.min(point.y);
        self.max_x = self.max_x.max(point.x);
        self.max_y = self.max_y.max(point.y);
    }

    /// Finishes accumulation and returns the bounding rectangle.
    ///
    /// Returns `None` if no points were included.
    pub fn finish(self) -> Option<Rect> {
        if self.min_x.is_finite()
            && self.min_y.is_finite()
            && self.max_x.is_finite()
            && self.max_y.is_finite()
        {
            Some(Rect {
                left: self.min_x,
                top: self.min_y,
                right: self.max_x,
                bottom: self.max_y,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BoundsAccumulator;
    use crate::{point::Point, rect::Rect};

    #[test]
    fn finish_returns_none_when_empty() {
        assert_eq!(BoundsAccumulator::new().finish(), None);
    }

    #[test]
    fn include_tracks_a_single_point() {
        let mut bounds = BoundsAccumulator::new();
        bounds.include(Point::new(12.5, -3.25));

        assert_eq!(
            bounds.finish(),
            Some(Rect {
                left: 12.5,
                top: -3.25,
                right: 12.5,
                bottom: -3.25,
            })
        );
    }

    #[test]
    fn include_tracks_min_and_max_points() {
        let mut bounds = BoundsAccumulator::new();
        bounds.include(Point::new(4.0, 9.0));
        bounds.include(Point::new(-6.5, 1.0));
        bounds.include(Point::new(2.0, -8.0));
        bounds.include(Point::new(7.25, 3.5));

        assert_eq!(
            bounds.finish(),
            Some(Rect {
                left: -6.5,
                top: -8.0,
                right: 7.25,
                bottom: 9.0,
            })
        );
    }
}
