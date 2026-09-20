//! Backend-neutral vector outlines for annotations without native controls.
//!
//! Shapes use annotation-local top-left units so a host can draw them into the
//! entry's `size` box without interpreting quads, strokes, or line endings.
use crate::{
    kind::AnnotationKind as Kind,
    models::{Annotation, Point},
    pdf_data::LineEndingStyle,
    style::ResolvedStyle,
};
use pdf_graphics::{color::Color, polyline::Polyline, rect::Rect};
use serde::{Deserialize, Serialize};

/// Fill alpha of highlight quadrilaterals.
const HIGHLIGHT_ALPHA: f32 = 0.35;
/// Smallest line-ending decoration.
const MIN_DECORATION_SIZE: f64 = 3.0;
/// Line-ending decoration size relative to the stroke width.
const DECORATION_STROKE_SCALE: f64 = 3.0;

pub use crate::style::ShapePaint;

/// One primitive in annotation-local top-left units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Shape {
    /// Open polyline or closed polygon; a straight segment is two open vertices.
    Polyline {
        /// Vertices in drawing order and whether the outline closes back to the first.
        path: Polyline<f64>,
        /// Paint for the outline and interior.
        paint: ShapePaint,
    },
    /// Axis-aligned ellipse inscribed in its bounds.
    Ellipse {
        /// Enclosing rectangle.
        bounds: Rect<f64>,
        /// Paint for the outline and interior.
        paint: ShapePaint,
    },
    /// Axis-aligned rectangle.
    Rect {
        /// Rectangle edges.
        bounds: Rect<f64>,
        /// Paint for the outline and interior.
        paint: ShapePaint,
    },
}

/// Converts a page point into local top-left units of `bounds`.
fn local(bounds: &Rect<f64>, point: Point) -> Point {
    Point::new(point.x - bounds.left, bounds.bottom - point.y)
}

/// Rectangle inset from the local box of `bounds` by half the stroke width.
fn inset_rect(bounds: &Rect<f64>, style: &ResolvedStyle) -> Rect<f64> {
    let inset = style.stroke_width() / 2.0;
    Rect {
        left: inset,
        top: inset,
        right: (bounds.width() - inset).max(inset),
        bottom: (bounds.height() - inset).max(inset),
    }
}

/// Style colors are opaque by contract; the annotation opacity is applied by the host.
fn opaque(color: Color) -> Color {
    Color { a: 1.0, ..color }
}

/// Builds the local shapes for `annotation`, or none for kinds with native presentation.
pub(crate) fn shapes(
    annotation: &Annotation,
    bounds: &Rect<f64>,
    style: &ResolvedStyle,
) -> Vec<Shape> {
    match &annotation.content {
        Kind::Highlight(v) => v
            .quads
            .iter()
            .map(|quad| Shape::Polyline {
                path: Polyline::from(quad).map(|corner| local(bounds, corner)),
                paint: ShapePaint {
                    stroke: None,
                    fill: Some(Color {
                        a: HIGHLIGHT_ALPHA,
                        ..style.color
                    }),
                    width: 0.0,
                    dash: Vec::new(),
                    round: false,
                },
            })
            .collect(),
        Kind::Underline(v) | Kind::Squiggly(v) => markup(
            &v.quads,
            bounds,
            style,
            1.0,
            matches!(annotation.content, Kind::Squiggly(_)),
        ),
        Kind::StrikeOut(v) => markup(&v.quads, bounds, style, 0.5, false),
        Kind::Ink(v) => v
            .strokes
            .iter()
            .map(|stroke| Shape::Polyline {
                path: stroke.map(|p| local(bounds, p)),
                paint: ShapePaint {
                    round: true,
                    ..style.outline(false)
                },
            })
            .collect(),
        Kind::Circle(_) => vec![Shape::Ellipse {
            bounds: inset_rect(bounds, style),
            paint: style.outline(true),
        }],
        Kind::Square(_) => vec![Shape::Rect {
            bounds: inset_rect(bounds, style),
            paint: style.outline(true),
        }],
        Kind::Line(v) | Kind::PolyLine(v) | Kind::Polygon(v) => {
            path(&v.vertices, v.endings.as_ref(), bounds, style)
        }
        _ => Vec::new(),
    }
}

/// Draws one decoration per quad along the line `ratio` of the way from top to bottom.
fn markup(
    quads: &[crate::models::Quad],
    bounds: &Rect<f64>,
    style: &ResolvedStyle,
    ratio: f64,
    squiggly: bool,
) -> Vec<Shape> {
    quads
        .iter()
        .map(|quad| {
            let [tl, tr, br, bl] = quad.corners.map(|corner| local(bounds, corner));
            let a = lerp(tl, bl, ratio);
            let b = lerp(tr, br, ratio);
            let path = if squiggly {
                Polyline::squiggle(a, b)
            } else {
                Polyline::new(vec![a, b], false)
            };
            Shape::Polyline {
                path,
                paint: style.outline(false),
            }
        })
        .collect()
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

/// Path outline plus its endpoint decorations.
fn path(
    vertices: &Polyline<f64>,
    endings: Option<&[LineEndingStyle; 2]>,
    bounds: &Rect<f64>,
    style: &ResolvedStyle,
) -> Vec<Shape> {
    let path = vertices.map(|p| local(bounds, p));
    let mut decorations = Vec::new();
    // Each end is oriented by its neighbour, so two vertices suffice for both decorations.
    if let (Some([start, end]), [first, second, ..], [.., penultimate, last]) =
        (endings, path.points(), path.points())
    {
        decorations.extend(ending(start, *first, *second, style));
        decorations.extend(ending(end, *last, *penultimate, style));
    }
    let mut shapes = vec![Shape::Polyline {
        paint: style.outline(path.is_closed()),
        path,
    }];
    shapes.append(&mut decorations);
    shapes
}

/// Decoration at `end`, oriented away from `adjacent`, in local units.
fn ending(
    kind: &LineEndingStyle,
    end: Point,
    adjacent: Point,
    style: &ResolvedStyle,
) -> Option<Shape> {
    let size = (style.stroke_width() * DECORATION_STROKE_SCALE).max(MIN_DECORATION_SIZE);
    let half = size / 2.0;
    let (sin, cos) = (end.y - adjacent.y).atan2(end.x - adjacent.x).sin_cos();
    let place = |[x, y]: [f64; 2]| Point::new(end.x + x * cos - y * sin, end.y + x * sin + y * cos);
    let filled = ShapePaint {
        fill: Some(opaque(style.color)),
        ..style.outline(false)
    };
    let closed = |points: &[[f64; 2]]| Shape::Polyline {
        path: Polyline::new(points.iter().map(|p| place(*p)).collect(), true),
        paint: filled.clone(),
    };
    let open = |points: &[[f64; 2]]| Shape::Polyline {
        path: Polyline::new(points.iter().map(|p| place(*p)).collect(), false),
        paint: style.outline(false),
    };
    Some(match kind {
        LineEndingStyle::Circle => Shape::Ellipse {
            bounds: Rect {
                left: end.x - half,
                top: end.y - half,
                right: end.x + half,
                bottom: end.y + half,
            },
            paint: filled,
        },
        LineEndingStyle::Square => {
            closed(&[[-half, -half], [half, -half], [half, half], [-half, half]])
        }
        LineEndingStyle::Diamond => closed(&[[0.0, -half], [half, 0.0], [0.0, half], [-half, 0.0]]),
        LineEndingStyle::Butt => open(&[[0.0, -half], [0.0, half]]),
        LineEndingStyle::Slash => open(&[[-half, -half], [half, half]]),
        LineEndingStyle::ClosedArrow => closed(&[[-size, -half], [0.0, 0.0], [-size, half]]),
        LineEndingStyle::OpenArrow => open(&[[-size, -half], [0.0, 0.0], [-size, half]]),
        LineEndingStyle::None | LineEndingStyle::Unknown(_) => return None,
    })
}
