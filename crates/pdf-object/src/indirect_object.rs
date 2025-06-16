use crate::ObjectVariant;

/// Represents an indirect object in a PDF file.
/// An indirect object is a data structure that can be referenced by other objects.
#[derive(Debug, PartialEq, Clone)]
pub struct IndirectObject {
    /// The object number, identifying this stream as an indirect object.
    pub object_number: i32,
    /// The generation number, used for PDF incremental updates.
    pub generation_number: i32,
    /// The object associated with this indirect object.
    pub object: Option<ObjectVariant>,
}

impl IndirectObject {
    pub fn new(object_number: i32, generation_number: i32, object: Option<ObjectVariant>) -> Self {
        IndirectObject {
            object_number,
            generation_number,
            object,
        }
    }
}
