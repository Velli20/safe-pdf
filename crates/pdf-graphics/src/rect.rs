use crate::{point::Point, transform::Transform};

/// An axis-aligned rectangle whose edge coordinate type defaults to `f32`.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Rect<T = f32> {
    /// The left edge of the rectangle.
    pub left: T,
    /// The top edge of the rectangle.
    pub top: T,
    /// The right edge of the rectangle.
    pub right: T,
    /// The bottom edge of the rectangle.
    pub bottom: T,
}

impl Rect<f32> {
    pub const UNIT_RECT: Rect = Rect {
        left: 0.0,
        top: 0.0,
        right: 1.0,
        bottom: 1.0,
    };

    /// Creates a new rectangle with the given width and height, with the top-left corner at (0, 0).
    pub const fn new(width: f32, height: f32) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            right: width,
            bottom: height,
        }
    }

    /// Returns a rectangle whose edges are ordered so width and height are non-negative.
    pub fn normalized(&self) -> Self {
        Self {
            left: self.left.min(self.right),
            top: self.top.min(self.bottom),
            right: self.left.max(self.right),
            bottom: self.top.max(self.bottom),
        }
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// Returns whether this rectangle has finite edges and positive dimensions.
    pub fn is_valid(&self) -> bool {
        self.left.is_finite()
            && self.top.is_finite()
            && self.right.is_finite()
            && self.bottom.is_finite()
            && self.width() > 0.0
            && self.height() > 0.0
    }

    /// Returns whether a point lies inside or on this rectangle's boundary.
    pub fn contains(&self, point: Point) -> bool {
        let rect = self.normalized();
        point.x >= rect.left
            && point.x <= rect.right
            && point.y >= rect.top
            && point.y <= rect.bottom
    }

    /// Returns the scale transform produced by dividing this rectangle's size by another.
    pub fn scale(&self, other: &Self) -> Transform {
        Transform::from_scale(self.width() / other.width(), self.height() / other.height())
    }
}

impl Rect<usize> {
    /// Creates an integer rectangle with the given size and its top-left corner at the origin.
    pub const fn from_size(width: usize, height: usize) -> Self {
        Self {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }
    }

    /// Returns the rectangle width without underflowing for inverted edges.
    pub const fn width(&self) -> usize {
        self.right.saturating_sub(self.left)
    }

    /// Returns the rectangle height without underflowing for inverted edges.
    pub const fn height(&self) -> usize {
        self.bottom.saturating_sub(self.top)
    }

    /// Returns whether the rectangle has positive width and height.
    pub const fn is_valid(&self) -> bool {
        self.width() > 0 && self.height() > 0
    }
}

impl From<[f32; 4]> for Rect {
    fn from(arr: [f32; 4]) -> Self {
        Rect {
            left: arr[0],
            top: arr[1],
            right: arr[2],
            bottom: arr[3],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{point::Point, transform::Transform};

    use super::Rect;

    #[test]
    fn normalized_orders_inverted_edges() {
        let rect = Rect {
            left: 10.0,
            top: 20.0,
            right: -5.0,
            bottom: -1.0,
        };

        assert_eq!(
            rect.normalized(),
            Rect {
                left: -5.0,
                top: -1.0,
                right: 10.0,
                bottom: 20.0,
            }
        );
    }

    #[test]
    fn is_valid_requires_finite_positive_dimensions() {
        assert!(Rect::new(10.0, 20.0).is_valid());
        assert!(!Rect::new(0.0, 20.0).is_valid());
        assert!(!Rect::new(10.0, 0.0).is_valid());
        assert!(!Rect::new(-10.0, 20.0).is_valid());

        let rect = Rect {
            left: 0.0,
            top: 0.0,
            right: f32::INFINITY,
            bottom: 20.0,
        };
        assert!(!rect.is_valid());
    }

    #[test]
    fn contains_includes_boundaries_and_normalizes_edges() {
        let rect = Rect {
            left: 10.0,
            top: 20.0,
            right: 0.0,
            bottom: 5.0,
        };

        assert!(rect.contains(Point::new(5.0, 10.0)));
        assert!(rect.contains(Point::new(0.0, 5.0)));
        assert!(rect.contains(Point::new(10.0, 20.0)));
        assert!(!rect.contains(Point::new(-1.0, 10.0)));
        assert!(!rect.contains(Point::new(5.0, 21.0)));
    }

    #[test]
    fn scale_returns_size_ratio_transform() {
        let rect = Rect::new(40.0, 15.0);
        let other = Rect::new(10.0, 5.0);

        assert_eq!(rect.scale(&other), Transform::from_scale(4.0, 3.0));
    }

    #[test]
    fn integer_rect_from_size_uses_origin_and_reports_dimensions() {
        let rect = Rect::<usize>::from_size(40, 15);

        assert_eq!(
            rect,
            Rect {
                left: 0,
                top: 0,
                right: 40,
                bottom: 15,
            }
        );
        assert_eq!(rect.width(), 40);
        assert_eq!(rect.height(), 15);
        assert!(rect.is_valid());
    }

    #[test]
    fn integer_rect_dimensions_saturate_for_inverted_edges() {
        let rect = Rect {
            left: 10_usize,
            top: 20_usize,
            right: 5_usize,
            bottom: 15_usize,
        };

        assert_eq!(rect.width(), 0);
        assert_eq!(rect.height(), 0);
        assert!(!rect.is_valid());
        assert!(!Rect::<usize>::from_size(0, 15).is_valid());
        assert!(!Rect::<usize>::from_size(40, 0).is_valid());
    }
}
