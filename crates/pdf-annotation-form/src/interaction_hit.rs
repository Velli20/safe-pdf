//! A hit annotation and its activation behavior.

use pdf_annotation_types::annotation_id::AnnotationId;
use pdf_document::document::PdfDocument;

use crate::{WidgetEditError, WidgetEditor};

/// A hit annotation and any listbox option under the pointer.
#[derive(Clone, Copy, Debug)]
pub(super) struct AnnotationHit {
    /// Stable identifier of the hit annotation.
    id: AnnotationId,
    /// Visible listbox option under the pointer, when applicable.
    listbox_option: Option<usize>,
}

impl AnnotationHit {
    /// Creates a hit for an annotation and optional listbox option.
    pub(super) const fn new(id: AnnotationId, listbox_option: Option<usize>) -> Self {
        Self { id, listbox_option }
    }

    /// Returns the hit annotation identifier.
    pub(super) const fn id(self) -> AnnotationId {
        self.id
    }

    /// Activates a button or listbox option represented by this hit.
    pub(super) fn activate(
        self,
        document: &mut PdfDocument,
        page_index: usize,
    ) -> Result<bool, WidgetEditError> {
        let mut editor = WidgetEditor::new(document);
        if editor.activate(page_index, self.id)?.is_some() {
            return Ok(true);
        }
        let Some(option_index) = self.listbox_option else {
            return Ok(false);
        };
        Ok(editor
            .activate_listbox_option(page_index, self.id, option_index)?
            .is_some())
    }
}
