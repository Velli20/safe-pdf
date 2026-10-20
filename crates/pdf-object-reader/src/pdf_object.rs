//! Shared handles for PDF values.

use crate::{object_id::ObjectId, object_kind::ObjectKind, object_variant::ObjectVariant};
use std::sync::Arc;

/// Provides cheap shared ownership of one immutable PDF value.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfObject(Arc<ObjectVariant>);

impl PdfObject {
    /// Creates a shared object handle around a PDF value.
    pub fn new(value: ObjectVariant) -> Self {
        Self(Arc::new(value))
    }

    /// Creates an indirect-reference object.
    pub fn reference(object_id: ObjectId) -> Self {
        Self::new(ObjectVariant::Reference(object_id))
    }

    /// Returns the immutable value stored by this object handle.
    pub fn value(&self) -> &ObjectVariant {
        &self.0
    }

    /// Returns the runtime kind of this object.
    pub fn kind(&self) -> ObjectKind {
        self.value().kind()
    }
}
