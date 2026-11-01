//! Identity lookups shared by the engine, its read-only view, and validation.
use crate::fields::{Field, FieldId};
use crate::models::{Annotation, AnnotationId, DocumentData};

impl DocumentData {
    /// Borrow an annotation by identity, returning `None` when it is absent.
    pub(crate) fn annotation(&self, id: AnnotationId) -> Option<&Annotation> {
        self.annotations
            .iter()
            .find(|annotation| annotation.id == id)
    }

    /// Mutably borrow an annotation by identity, returning `None` when it is absent.
    pub(crate) fn annotation_mut(&mut self, id: AnnotationId) -> Option<&mut Annotation> {
        self.annotations
            .iter_mut()
            .find(|annotation| annotation.id == id)
    }

    /// Borrow a field by identity, returning `None` when it is absent.
    pub(crate) fn field(&self, id: FieldId) -> Option<&Field> {
        self.fields.iter().find(|field| field.id == id)
    }

    /// Mutably borrow a field by identity, returning `None` when it is absent.
    pub(crate) fn field_mut(&mut self, id: FieldId) -> Option<&mut Field> {
        self.fields.iter_mut().find(|field| field.id == id)
    }
}
