//! Validated listbox geometry and row lookup.

use pdf_annotation_types::{Annotation, AnnotationKind, WidgetAnnotation};
use pdf_graphics::rect::Rect;

use crate::{
    interaction_listbox_metrics::ListboxMetrics, interaction_viewport::AnnotationViewport,
    interaction_visible_listbox_rows::VisibleListboxRows,
};

/// Validated geometry needed to enumerate visible listbox rows.
#[derive(Clone, Copy, Debug)]
pub(super) struct ListboxLayout {
    /// Total number of widget options.
    option_count: usize,
    /// First option displayed by the widget.
    top_index: usize,
    /// Device-space widget bounds.
    rect: Rect,
    /// Height of one row in device units.
    row_height: f32,
}

impl ListboxLayout {
    /// Creates a layout for a listbox widget and mapped annotation rectangle.
    pub(super) fn new(
        widget: &WidgetAnnotation,
        rect: Rect,
        viewport: AnnotationViewport,
    ) -> Option<Self> {
        let option_count = widget.options.as_ref()?.len();
        let top_index = widget.top_index.unwrap_or(0).min(option_count);
        let row_height = ListboxMetrics::new(widget).device_row_height(viewport)?;
        Some(Self {
            option_count,
            top_index,
            rect,
            row_height,
        })
    }

    /// Creates a layout from a listbox annotation and viewport.
    pub(super) fn from_annotation(
        annotation: &Annotation,
        viewport: AnnotationViewport,
    ) -> Option<Self> {
        let AnnotationKind::Widget(widget) = &annotation.kind else {
            return None;
        };
        if !widget.is_listbox() {
            return None;
        }
        let rect = viewport.map_rect(annotation.rect.as_ref()?)?;
        Self::new(widget, rect, viewport)
    }

    /// Returns an iterator over rows intersecting the widget rectangle.
    pub(super) const fn rows(self) -> VisibleListboxRows {
        VisibleListboxRows::new(
            self.top_index,
            self.option_count,
            self.rect,
            self.row_height,
        )
    }

    /// Returns the option index containing a device-space vertical position.
    pub(super) fn option_at(self, position_y: f32) -> Option<usize> {
        self.rows().find_map(|row| {
            (position_y >= row.rect.top && position_y <= row.rect.bottom)
                .then_some(row.option_index)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_rows_support_top_indices_larger_than_u16() {
        let layout = ListboxLayout {
            option_count: 70_005,
            top_index: 70_000,
            rect: Rect {
                left: 10.0,
                top: 20.0,
                right: 110.0,
                bottom: 45.0,
            },
            row_height: 12.0,
        };
        let rows = layout.rows().collect::<Vec<_>>();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows.first().map(|row| row.option_index), Some(70_000));
        assert_eq!(rows.last().map(|row| row.option_index), Some(70_002));
        assert_eq!(rows.last().map(|row| row.rect.bottom), Some(45.0));
    }
}
