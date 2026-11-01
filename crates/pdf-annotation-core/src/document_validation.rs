//! Structural validation of document data before it becomes live engine state.
use std::collections::HashSet;

use crate::error::{CoreError, ValidationError};
use crate::kind::AnnotationKind;
use crate::models::{DocumentData, PageIndex};

impl DocumentData {
    /// Validate a complete document: finite numbers, allocation counters, unique
    /// identities, field definitions, page indices, payloads, popup parents, and
    /// widget bindings.
    pub(crate) fn validate<E: std::error::Error + 'static>(&self) -> Result<(), CoreError<E>> {
        crate::finite_values::check(self)?;
        self.validate_fields()?;
        self.validate_annotations()
    }

    /// Validate `content` placed on `page` against the page count, its own
    /// payload rules, and the popup parent or field it references. The caller has
    /// already checked its numbers with [`crate::finite_values::check`].
    pub(crate) fn validate_content<E: std::error::Error + 'static>(
        &self,
        page: PageIndex,
        content: &AnnotationKind,
    ) -> Result<(), CoreError<E>> {
        if page.0 >= self.document.page_count {
            return Err(ValidationError::InvalidPage {
                page,
                page_count: self.document.page_count,
            }
            .into());
        }
        content.validate()?;
        if let Some(parent) = content.popup_parent() {
            let parent = self
                .annotation(parent)
                .ok_or(CoreError::InvalidPopupParent(parent))?;
            if parent.page != page || matches!(parent.content, AnnotationKind::Popup(_)) {
                return Err(CoreError::InvalidPopupParent(parent.id));
            }
        }
        if let AnnotationKind::Widget(widget) = content {
            let id = widget.binding.field();
            let field = self.field(id).ok_or(CoreError::MissingField(id))?;
            widget.binding.validate(field)?;
        }
        Ok(())
    }

    fn validate_fields<E: std::error::Error + 'static>(&self) -> Result<(), CoreError<E>> {
        if self.next_field_id == 0 {
            return Err(ValidationError::InvalidNextFieldId.into());
        }
        let mut ids = HashSet::new();
        for field in &self.fields {
            field.definition.validate()?;
            if !ids.insert(field.id.0) {
                return Err(ValidationError::DuplicateFieldId(field.id).into());
            }
            if field.id.0 >= self.next_field_id {
                return Err(ValidationError::InvalidNextFieldId.into());
            }
        }
        Ok(())
    }

    fn validate_annotations<E: std::error::Error + 'static>(&self) -> Result<(), CoreError<E>> {
        if self.next_annotation_id == 0 {
            return Err(ValidationError::InvalidNextId.into());
        }
        let mut ids = HashSet::new();
        for annotation in &self.annotations {
            if !ids.insert(annotation.id.0) {
                return Err(ValidationError::DuplicateId(annotation.id).into());
            }
            if annotation.id.0 >= self.next_annotation_id {
                return Err(ValidationError::InvalidNextId.into());
            }
            self.validate_content(annotation.page, &annotation.content)?;
        }
        Ok(())
    }
}
