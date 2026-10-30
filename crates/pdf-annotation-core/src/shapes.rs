//! Backend-neutral vector outlines for annotations without native controls.
//!
//! Shapes use annotation-local top-left units so a host can draw them into the
//! entry's `size` box without interpreting quads, strokes, or line endings.
use crate::{
    kind::AnnotationKind as Kind, models::Annotation, shape_markup::MarkupLine,
    style::ResolvedStyle,
};
use pdf_graphics::{polyline::Polyline, rect::Rect};
use serde::{Deserialize, Serialize};

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
    /// Axis-aligned rectangle.
    Rect {
        /// Rectangle edges.
        bounds: Rect,
        /// Paint for the outline and interior.
        paint: ShapePaint,
    },
}

/// Builds the local shapes for `annotation`, or none for kinds with native presentation.
pub(crate) fn shapes(annotation: &Annotation, bounds: &Rect, style: &ResolvedStyle) -> Vec<Shape> {
    match &annotation.content {
        Kind::Highlight(v) => v.shapes(bounds, style),
        Kind::Underline(v) => v.shapes(bounds, style, MarkupLine::Underline),
        Kind::Squiggly(v) => v.shapes(bounds, style, MarkupLine::Squiggly),
        Kind::StrikeOut(v) => v.shapes(bounds, style, MarkupLine::StrikeOut),
        Kind::Ink(v) => v.shapes(bounds, style),
        Kind::Circle(_) | Kind::Square(_) => {
            let bounds = bounds.local_inset(style.stroke_width() / 2.0);
            let paint = style.outline(true);
            vec![if matches!(annotation.content, Kind::Circle(_)) {
                Shape::Ellipse { bounds, paint }
            } else {
                Shape::Rect { bounds, paint }
            }]
        }
        Kind::Line(v) | Kind::PolyLine(v) | Kind::Polygon(v) => v.shapes(bounds, style),
        _ => Vec::new(),
    }
}
