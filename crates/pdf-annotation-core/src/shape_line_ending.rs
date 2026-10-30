//! Local shapes for the decorations drawn at path endpoints.
use crate::{
    models::Point,
    pdf_data::LineEndingStyle,
    shapes::{Shape, ShapePaint},
    style::ResolvedStyle,
};
use pdf_graphics::{polyline::Polyline, rect::Rect, transform::Transform};

/// Smallest line-ending decoration.
const MIN_DECORATION_SIZE: f32 = 3.0;
/// Line-ending decoration size relative to the stroke width.
const DECORATION_STROKE_SCALE: f32 = 3.0;

impl LineEndingStyle {
    /// Decoration at `end`, oriented away from `adjacent`, in local units; `None`
    /// for styles that draw nothing.
    pub(crate) fn decoration(
        &self,
        end: Point,
        adjacent: Point,
        style: &ResolvedStyle,
    ) -> Option<Shape> {
        let size = (style.stroke_width() * DECORATION_STROKE_SCALE).max(MIN_DECORATION_SIZE);
        let half = size / 2.0;
        let (sin, cos) = (end.y - adjacent.y).atan2(end.x - adjacent.x).sin_cos();
        // Vertices are laid out with `end` at the origin and the line direction along +x.
        let frame = Transform::from_row(cos, sin, -sin, cos, end.x, end.y);
        let filled = || ShapePaint {
            fill: Some(style.color.opaque()),
            ..style.outline(false)
        };
        let square = Rect {
            left: -half,
            top: -half,
            right: half,
            bottom: half,
        };
        let mut path = match self {
            Self::Circle => {
                // Rotation leaves a circle unchanged, so only the translation applies.
                let mut bounds = square;
                bounds.translate(end.x, end.y);
                return Some(Shape::Ellipse {
                    bounds,
                    paint: filled(),
                });
            }
            Self::Square => Polyline::from(&square),
            Self::Diamond => Polyline::new(
                vec![
                    Point::new(0.0, -half),
                    Point::new(half, 0.0),
                    Point::new(0.0, half),
                    Point::new(-half, 0.0),
                ],
                true,
            ),
            Self::Butt => Polyline::new(vec![Point::new(0.0, -half), Point::new(0.0, half)], false),
            Self::Slash => Polyline::new(
                vec![Point::new(-half, -half), Point::new(half, half)],
                false,
            ),
            Self::ClosedArrow | Self::OpenArrow => Polyline::new(
                vec![
                    Point::new(-size, -half),
                    Point::new(0.0, 0.0),
                    Point::new(-size, half),
                ],
                matches!(self, Self::ClosedArrow),
            ),
            Self::None | Self::Unknown(_) => return None,
        };
        path.transform(&frame);
        Some(Shape::Polyline {
            paint: if path.is_closed() {
                filled()
            } else {
                style.outline(false)
            },
            path,
        })
    }
}
