//! Backend-neutral vector outlines for annotations without native controls.
//!
//! Shapes use annotation-local top-left units so a host can draw them into the
//! entry's `size` box without interpreting quads, strokes, or line endings.
use crate::{
    kind::AnnotationKind as Kind,
    models::{Annotation, Point, Quad},
    pdf_data::LineEndingStyle,
    style::ResolvedStyle,
    subtypes::PathAnnotation,
};
use pdf_graphics::{Translate, color::Color, polyline::Polyline, rect::Rect, transform::Transform};
use serde::{Deserialize, Serialize};

/// Fill alpha of highlight quadrilaterals.
const HIGHLIGHT_ALPHA: f32 = 0.35;
/// Smallest line-ending decoration.
const MIN_DECORATION_SIZE: f32 = 3.0;
/// Line-ending decoration size relative to the stroke width.
const DECORATION_STROKE_SCALE: f32 = 3.0;

/// Fill and stroke of one shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct ShapePaint {
    /// Outline color whose alpha is the outline opacity; `None` draws no outline.
    pub stroke: Option<Color>,
    /// Interior color whose alpha is the interior opacity; `None` leaves the interior unpainted.
    pub fill: Option<Color>,
    /// Outline width in local units.
    pub width: f32,
    /// Dash lengths; `None` is solid.
    pub dash: Option<Vec<f32>>,
    /// Round caps and joins instead of the host default.
    pub round: bool,
}

impl ShapePaint {
    /// Outline in the style color at [`ResolvedStyle::stroke_width`] with its dash
    /// pattern, filled with `fill`. The host applies annotation opacity separately.
    fn outline(style: &ResolvedStyle, fill: Option<Color>) -> Self {
        Self {
            stroke: Some(style.color),
            fill,
            width: style.stroke_width(),
            dash: style.dash.clone(),
            round: false,
        }
    }
}

/// One primitive in annotation-local top-left units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Shape {
    /// Open polyline or closed polygon; a straight segment is two open vertices.
    Polyline {
        /// Vertices in drawing order and whether the outline closes back to the first.
        path: Polyline,
        /// Paint for the outline and interior.
        paint: ShapePaint,
    },
    /// Axis-aligned ellipse inscribed in its bounds.
    Ellipse {
        /// Enclosing rectangle.
        bounds: Rect,
        /// Paint for the outline and interior.
        paint: ShapePaint,
    },
}

/// Builds the local shapes for `annotation`, or none for kinds with native presentation.
pub(crate) fn shapes(annotation: &Annotation, bounds: &Rect, style: &ResolvedStyle) -> Vec<Shape> {
    // Page space to local top-left units: the origin is `(left, bottom)` and y grows towards `top`.
    let local = Transform::from_row(1.0, 0.0, 0.0, -1.0, -bounds.left, bounds.bottom);
    let line = ShapePaint::outline(style, None);
    let background = style.background;
    match &annotation.content {
        Kind::Highlight(v) => {
            let paint = ShapePaint {
                stroke: None,
                fill: Some(style.color.with_alpha(HIGHLIGHT_ALPHA)),
                width: 0.0,
                dash: None,
                round: false,
            };
            quad_shapes(&v.quads, &local, &paint, |quad| Polyline::from(quad))
        }
        Kind::Underline(v) => quad_shapes(&v.quads, &local, &line, |q| across(q, 1.0, false)),
        Kind::Squiggly(v) => quad_shapes(&v.quads, &local, &line, |q| across(q, 1.0, true)),
        Kind::StrikeOut(v) => quad_shapes(&v.quads, &local, &line, |q| across(q, 0.5, false)),
        Kind::Ink(v) => {
            let paint = ShapePaint {
                round: true,
                ..line
            };
            v.strokes
                .iter()
                .map(|stroke| Shape::Polyline {
                    path: local_path(stroke, &local),
                    paint: paint.clone(),
                })
                .collect()
        }
        Kind::Circle(_) | Kind::Square(_) => {
            let inset = style.stroke_width() / 2.0;
            let bounds = Rect {
                left: inset,
                top: inset,
                right: (bounds.width() - inset).max(inset),
                bottom: (bounds.height() - inset).max(inset),
            };
            let paint = ShapePaint::outline(style, background);
            vec![if matches!(annotation.content, Kind::Circle(_)) {
                Shape::Ellipse { bounds, paint }
            } else {
                Shape::Polyline {
                    path: Polyline::from(&bounds),
                    paint,
                }
            }]
        }
        Kind::Line(v) | Kind::PolyLine(v) | Kind::Polygon(v) => v.shapes(&local, style),
        _ => Vec::new(),
    }
}

/// One `paint`ed polyline per quad, built by `outline` in local corner coordinates.
fn quad_shapes(
    quads: &[Quad],
    local: &Transform,
    paint: &ShapePaint,
    outline: impl Fn(&Quad) -> Polyline,
) -> Vec<Shape> {
    quads
        .iter()
        .map(|quad| {
            let corners = quad.corners.map(|corner| {
                let (x, y) = local.transform_point(corner.x, corner.y);
                Point::new(x, y)
            });
            Shape::Polyline {
                path: outline(&Quad { corners }),
                paint: paint.clone(),
            }
        })
        .collect()
}

/// Line across a top-left, top-right, bottom-right, bottom-left quad at `ratio` of the way
/// from its top edge to its bottom edge, zigzagging when `squiggly`.
fn across(quad: &Quad, ratio: f32, squiggly: bool) -> Polyline {
    let [tl, tr, br, bl] = quad.corners;
    let (a, b) = (tl.lerp(bl, ratio), tr.lerp(br, ratio));
    if squiggly {
        Polyline::squiggle(a, b)
    } else {
        Polyline::new(vec![a, b], false)
    }
}

/// Copy of `path` in local units.
fn local_path(path: &Polyline, local: &Transform) -> Polyline {
    let mut path = path.clone();
    path.transform(local);
    path
}

impl PathAnnotation {
    /// Path outline followed by its endpoint decorations, in local units.
    fn shapes(&self, local: &Transform, style: &ResolvedStyle) -> Vec<Shape> {
        let path = local_path(&self.vertices, local);
        // Each end is oriented by its neighbour, so two vertices suffice for both decorations.
        let decorations = match (&self.endings, path.points(), path.points()) {
            (Some([start, end]), [first, second, ..], [.., penultimate, last]) => [
                start.decoration(*first, *second, style),
                end.decoration(*last, *penultimate, style),
            ],
            _ => [None, None],
        };
        let fill = path.is_closed().then_some(style.background).flatten();
        let mut shapes = Vec::with_capacity(3);
        shapes.push(Shape::Polyline {
            paint: ShapePaint::outline(style, fill),
            path,
        });
        shapes.extend(decorations.into_iter().flatten());
        shapes
    }
}

impl LineEndingStyle {
    /// Decoration at `end`, oriented away from `adjacent`, in local units; closed
    /// decorations are filled with the style color. `None` for styles that draw nothing.
    fn decoration(&self, end: Point, adjacent: Point, style: &ResolvedStyle) -> Option<Shape> {
        let size = (style.stroke_width() * DECORATION_STROKE_SCALE).max(MIN_DECORATION_SIZE);
        let half = size / 2.0;
        let paint = |closed: bool| ShapePaint::outline(style, closed.then_some(style.color));
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
                bounds.translate(end);
                return Some(Shape::Ellipse {
                    bounds,
                    paint: paint(true),
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
        // Vertices are laid out with `end` at the origin and the line direction along +x.
        let (sin, cos) = (end.y - adjacent.y).atan2(end.x - adjacent.x).sin_cos();
        path.transform(&Transform::from_row(cos, sin, -sin, cos, end.x, end.y));
        Some(Shape::Polyline {
            paint: paint(path.is_closed()),
            path,
        })
    }
}
