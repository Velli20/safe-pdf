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
        match self.value() {
            ObjectVariant::Null => ObjectKind::Null,
            ObjectVariant::Boolean(_) => ObjectKind::Boolean,
            ObjectVariant::Integer(_) => ObjectKind::Integer,
            ObjectVariant::Real(_) => ObjectKind::Real,
            ObjectVariant::LiteralString(_) | ObjectVariant::HexString(_) => ObjectKind::String,
            ObjectVariant::Name(_) => ObjectKind::Name,
            ObjectVariant::Array(_) => ObjectKind::Array,
            ObjectVariant::Dictionary(_) => ObjectKind::Dictionary,
            ObjectVariant::Stream(_) => ObjectKind::Stream,
            ObjectVariant::Reference(_) => ObjectKind::Reference,
            ObjectVariant::Trailer(_) => ObjectKind::Trailer,
            ObjectVariant::CrossReferenceTable(_) => ObjectKind::CrossReferenceTable,
            ObjectVariant::EndOfFile => ObjectKind::EndOfFile,
        }
    }
}
