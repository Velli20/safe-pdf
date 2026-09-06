//! Checked direct PDF object handles.

use crate::{
    error::ObjectReadError, object_kind::ObjectKind, object_variant::ObjectVariant,
    pdf_object::PdfObject,
};

/// Wraps an object whose top-level value is known not to be a reference.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedObject(PdfObject);

/// Validates a direct object without following references.
impl TryFrom<PdfObject> for ResolvedObject {
    type Error = ObjectReadError;

    fn try_from(object: PdfObject) -> Result<Self, Self::Error> {
        if let ObjectVariant::Reference(object_id) = object.value() {
            return Err(Self::Error::UnresolvedReference {
                object_id: *object_id,
            });
        }
        Ok(Self(object))
    }
}

impl ResolvedObject {
    /// Returns the resolved shared object handle.
    pub fn object(&self) -> &PdfObject {
        &self.0
    }

    /// Returns the resolved object's value.
    pub fn value(&self) -> &ObjectVariant {
        self.0.value()
    }

    /// Returns the kind of the resolved value.
    pub fn kind(&self) -> ObjectKind {
        self.0.kind()
    }
}
