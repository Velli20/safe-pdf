//! Interaction capabilities derived from annotations.

use pdf_annotation_types::{AnnotationKind, annotation_id::AnnotationId};
use pdf_document::page::PdfPage;
use pdf_graphics::rect::Rect;

/// Interaction capabilities derived from one annotation.
#[derive(Clone, Copy, Debug)]
pub(super) struct AnnotationTarget {
    /// Whether a double click may begin free-text editing.
    free_text: bool,
    /// Normalized draggable bounds, when the subtype can move.
    draggable_rect: Option<Rect>,
}

impl AnnotationTarget {
    /// Reads interaction capabilities from a page annotation.
    pub(super) fn from_page(page: &PdfPage, id: AnnotationId) -> Option<Self> {
        let annotation = page.annotation(id)?;
        let draggable_rect = annotation
            .kind
            .is_draggable()
            .then(|| annotation.rect.map(|rect| rect.normalized()))
            .flatten()
            .filter(Rect::is_valid);
        Some(Self {
            free_text: matches!(annotation.kind, AnnotationKind::FreeText(_)),
            draggable_rect,
        })
    }

    /// Reports whether the target supports free-text editing.
    pub(super) const fn is_free_text(self) -> bool {
        self.free_text
    }

    /// Returns normalized draggable bounds when movement is supported.
    pub(super) const fn draggable_rect(self) -> Option<Rect> {
        self.draggable_rect
    }
}
