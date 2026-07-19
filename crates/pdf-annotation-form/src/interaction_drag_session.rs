//! Bounded annotation dragging and subtype geometry translation.

use pdf_annotation_types::{Annotation, AnnotationKind, annotation_id::AnnotationId};
use pdf_document::document::PdfDocument;
use pdf_graphics::{point::Point, rect::Rect, transform::Transform};

use crate::{
    interaction_types::{AnnotationInteractionError, AnnotationPointerMove},
    interaction_viewport::AnnotationViewport,
};

/// Squared device-space distance required to activate a drag gesture.
const DRAG_THRESHOLD_SQUARED: f32 = 9.0;

/// Internal outcome of processing pointer movement during a drag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DragUpdate {
    /// Movement belongs to another page or no drag is applicable.
    Ignored,
    /// Movement was consumed before the drag threshold was crossed.
    Pending,
    /// An active drag consumed movement without changing annotation geometry.
    Active,
    /// Movement changed annotation geometry and requires a redraw.
    Redraw,
}

/// Captures immutable gesture origins and mutable activation state.
#[derive(Clone, Copy, Debug)]
pub(super) struct DragSession {
    /// Page on which the drag began.
    page_index: usize,
    /// Annotation being moved.
    annotation_id: AnnotationId,
    /// Device-space pointer position at press time.
    press_position: Point,
    /// Normalized annotation rectangle at press time.
    original_rect: Rect,
    /// Page bounds used to constrain movement.
    page_bounds: Rect,
    /// Whether pointer movement has crossed the drag threshold.
    active: bool,
}

impl DragSession {
    /// Creates a pending drag session for a draggable annotation.
    pub(super) const fn new(
        page_index: usize,
        annotation_id: AnnotationId,
        press_position: Point,
        original_rect: Rect,
        viewport: AnnotationViewport,
    ) -> Self {
        Self {
            page_index,
            annotation_id,
            press_position,
            original_rect,
            page_bounds: viewport.page_bounds,
            active: false,
        }
    }

    /// Applies movement to the annotation after validating the gesture.
    pub(super) fn update(
        &mut self,
        document: &mut PdfDocument,
        movement: AnnotationPointerMove,
    ) -> Result<DragUpdate, AnnotationInteractionError> {
        if self.page_index != movement.page_index {
            return Ok(DragUpdate::Ignored);
        }

        let device_delta = Point::new(
            movement.position.x - self.press_position.x,
            movement.position.y - self.press_position.y,
        );
        if !device_delta.x.is_finite() || !device_delta.y.is_finite() {
            return Ok(if self.active {
                DragUpdate::Active
            } else {
                DragUpdate::Pending
            });
        }
        if !self.active && !Self::crosses_activation_threshold(device_delta) {
            return Ok(DragUpdate::Pending);
        }
        self.active = true;

        let Some(page_delta) = movement.viewport.map_device_delta(device_delta) else {
            return Ok(DragUpdate::Active);
        };
        let moved_rect = BoundedTranslation::new(self.original_rect, self.page_bounds, page_delta)
            .translated_rect();
        let page = document.pages.get_mut(movement.page_index).ok_or(
            crate::WidgetEditError::PageNotFound {
                page_index: movement.page_index,
            },
        )?;
        let annotation = page.annotation_mut(self.annotation_id).ok_or_else(|| {
            AnnotationInteractionError::AnnotationNotFound {
                id: self.annotation_id.get(),
            }
        })?;
        if annotation.rect.as_ref().map(Rect::normalized) == Some(moved_rect) {
            return Ok(DragUpdate::Active);
        }

        let current_rect = annotation
            .rect
            .map(|rect| rect.normalized())
            .unwrap_or(self.original_rect);
        let applied_delta = Point::new(
            moved_rect.left - current_rect.left,
            moved_rect.top - current_rect.top,
        );
        AnnotationTranslation::new(annotation, applied_delta).apply();
        annotation.rect = Some(moved_rect);
        Ok(DragUpdate::Redraw)
    }

    /// Reports whether a device delta meets the configured drag threshold.
    fn crosses_activation_threshold(delta: Point) -> bool {
        delta.x * delta.x + delta.y * delta.y >= DRAG_THRESHOLD_SQUARED
    }
}

/// Calculates a rectangle translation constrained to page bounds.
#[derive(Clone, Copy, Debug)]
struct BoundedTranslation {
    /// Rectangle to translate.
    rect: Rect,
    /// Bounds that should contain the rectangle.
    bounds: Rect,
    /// Requested page-space movement.
    delta: Point,
}

impl BoundedTranslation {
    /// Creates a bounded translation calculation.
    const fn new(rect: Rect, bounds: Rect, delta: Point) -> Self {
        Self {
            rect,
            bounds,
            delta,
        }
    }

    /// Returns the translated rectangle after independently clamping both axes.
    fn translated_rect(self) -> Rect {
        let delta_x = AxisTranslation::new(
            self.rect.left,
            self.rect.right,
            self.bounds.left,
            self.bounds.right,
            self.delta.x,
        )
        .clamped_delta();
        let delta_y = AxisTranslation::new(
            self.rect.top,
            self.rect.bottom,
            self.bounds.top,
            self.bounds.bottom,
            self.delta.y,
        )
        .clamped_delta();
        Rect {
            left: self.rect.left + delta_x,
            top: self.rect.top + delta_y,
            right: self.rect.right + delta_x,
            bottom: self.rect.bottom + delta_y,
        }
    }
}

/// Calculates the permitted movement along one rectangle axis.
#[derive(Clone, Copy, Debug)]
struct AxisTranslation {
    /// Start of the annotation extent.
    rect_start: f32,
    /// End of the annotation extent.
    rect_end: f32,
    /// Start of the containing extent.
    bounds_start: f32,
    /// End of the containing extent.
    bounds_end: f32,
    /// Requested movement.
    delta: f32,
}

impl AxisTranslation {
    /// Creates an axis-specific translation calculation.
    const fn new(
        rect_start: f32,
        rect_end: f32,
        bounds_start: f32,
        bounds_end: f32,
        delta: f32,
    ) -> Self {
        Self {
            rect_start,
            rect_end,
            bounds_start,
            bounds_end,
            delta,
        }
    }

    /// Clamps movement or freezes an annotation larger than its bounds.
    fn clamped_delta(self) -> f32 {
        if self.rect_end - self.rect_start > self.bounds_end - self.bounds_start {
            return 0.0;
        }
        self.delta.clamp(
            self.bounds_start - self.rect_start,
            self.bounds_end - self.rect_end,
        )
    }
}

/// Applies a page-space translation to subtype-specific annotation geometry.
struct AnnotationTranslation<'a> {
    /// Annotation whose owned geometry will move.
    annotation: &'a mut Annotation,
    /// Translation applied to every supported coordinate.
    delta: Point,
}

impl<'a> AnnotationTranslation<'a> {
    /// Creates a translation over one mutable annotation.
    fn new(annotation: &'a mut Annotation, delta: Point) -> Self {
        Self { annotation, delta }
    }

    /// Translates geometry owned by the annotation subtype.
    fn apply(&mut self) {
        let delta = self.delta;
        match &mut self.annotation.kind {
            AnnotationKind::FreeText(free_text) => {
                if let Some(callout_line) = free_text.callout_line.as_mut() {
                    Self::translate_coordinate_list(callout_line, delta);
                }
            }
            AnnotationKind::Line(line) => Self::translate_coordinate_list(&mut line.line, delta),
            AnnotationKind::Polygon(polygon) => polygon
                .vertices
                .transform(&Transform::from_translate(self.delta.x, self.delta.y)),
            AnnotationKind::PolyLine(polyline) => polyline
                .vertices
                .transform(&Transform::from_translate(self.delta.x, self.delta.y)),
            AnnotationKind::Ink(ink) => {
                let transform = Transform::from_translate(self.delta.x, self.delta.y);
                for stroke in &mut ink.ink_list.strokes {
                    stroke.transform(&transform);
                }
            }
            AnnotationKind::Text(_)
            | AnnotationKind::Stamp(_)
            | AnnotationKind::Square(_)
            | AnnotationKind::Circle(_) => {}
            _ => {}
        }
    }

    /// Translates alternating horizontal and vertical coordinates.
    fn translate_coordinate_list(coordinates: &mut [f32], delta: Point) {
        for pair in coordinates.chunks_mut(2) {
            if let [x, y] = pair {
                *x += delta.x;
                *y += delta.y;
            } else if let [x] = pair {
                *x += delta.x;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_annotation_axis_does_not_move() {
        let rect = Rect {
            left: -10.0,
            top: 20.0,
            right: 210.0,
            bottom: 40.0,
        };
        let bounds = Rect {
            left: 0.0,
            top: 0.0,
            right: 200.0,
            bottom: 100.0,
        };

        let moved = BoundedTranslation::new(rect, bounds, Point::new(50.0, 10.0)).translated_rect();

        assert_eq!(moved.left, rect.left);
        assert_eq!(moved.right, rect.right);
        assert_eq!(moved.top, 30.0);
        assert_eq!(moved.bottom, 50.0);
    }
}
