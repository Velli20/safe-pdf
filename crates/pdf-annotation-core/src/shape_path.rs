//! Local shapes for freehand strokes and vertex paths.
use crate::{
    models::Ink,
    shapes::{Shape, ShapePaint},
    style::ResolvedStyle,
    subtypes::PathAnnotation,
};
use pdf_graphics::rect::Rect;

impl Ink {
    /// One round-capped open outline per stroke, in local units of `bounds`.
    pub(crate) fn shapes(&self, bounds: &Rect, style: &ResolvedStyle) -> Vec<Shape> {
        let paint = ShapePaint {
            round: true,
            ..style.outline(false)
        };
        self.strokes
            .iter()
            .map(|stroke| Shape::Polyline {
                path: stroke.map(|point| bounds.to_local(point)),
                paint: paint.clone(),
            })
            .collect()
    }
}

impl PathAnnotation {
    /// Path outline followed by its endpoint decorations, in local units of `bounds`.
    pub(crate) fn shapes(&self, bounds: &Rect, style: &ResolvedStyle) -> Vec<Shape> {
        let path = self.vertices.map(|point| bounds.to_local(point));
        // Each end is oriented by its neighbour, so two vertices suffice for both decorations.
        let decorations = match (&self.endings, path.points(), path.points()) {
            (Some([start, end]), [first, second, ..], [.., penultimate, last]) => [
                start.decoration(*first, *second, style),
                end.decoration(*last, *penultimate, style),
            ],
            _ => [None, None],
        };
        let mut shapes = Vec::with_capacity(3);
        shapes.push(Shape::Polyline {
            paint: style.outline(path.is_closed()),
            path,
        });
        shapes.extend(decorations.into_iter().flatten());
        shapes
    }
}
