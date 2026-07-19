//! Device-space hit testing for interactive annotations.

use pdf_annotation_types::{Annotation, AnnotationKind, WidgetAnnotation};
use pdf_document::page::PdfPage;
use pdf_graphics::point::Point;

use crate::{
    interaction_hit::AnnotationHit, interaction_listbox_layout::ListboxLayout,
    interaction_viewport::AnnotationViewport,
};

/// Searches interactive annotations in reverse paint order.
pub(super) struct AnnotationHitTester<'a> {
    /// Page whose annotations will be searched.
    page: &'a PdfPage,
    /// Page-to-device mapping used for bounds checks.
    viewport: AnnotationViewport,
}

impl<'a> AnnotationHitTester<'a> {
    /// Creates a hit tester over one page and viewport.
    pub(super) const fn new(page: &'a PdfPage, viewport: AnnotationViewport) -> Self {
        Self { page, viewport }
    }

    /// Returns the topmost interactive annotation under a device position.
    pub(super) fn hit_at(&self, position: Point) -> Option<AnnotationHit> {
        self.page
            .annotations
            .as_deref()?
            .iter()
            .rev()
            .find_map(|annotation| self.hit_annotation(annotation, position))
    }

    /// Tests one annotation and derives listbox row information when needed.
    fn hit_annotation(&self, annotation: &Annotation, position: Point) -> Option<AnnotationHit> {
        let widget = match &annotation.kind {
            AnnotationKind::Widget(widget) => Some(widget),
            _ => None,
        };
        let listbox = widget.filter(|widget| widget.is_listbox());
        let interactive = annotation.kind.is_draggable()
            || widget.is_some_and(WidgetAnnotation::is_button)
            || listbox.is_some();
        if !interactive {
            return None;
        }

        let rect = self.viewport.map_rect(annotation.rect.as_ref()?)?;
        if !rect.contains(position) {
            return None;
        }
        Some(AnnotationHit::new(
            annotation.id(),
            listbox.and_then(|widget| {
                ListboxLayout::new(widget, rect, self.viewport)?.option_at(position.y)
            }),
        ))
    }
}
