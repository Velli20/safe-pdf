use crate::Value;

/// Represents an indirect object or a reference to an object in a PDF file.
/// An indirect object is a data structure that can be referenced by other objects.
/// A reference consists of an object number and a generation number.
#[derive(Debug, PartialEq)]
pub struct IndirectObjectOrReference {
    pub object_number: i32,
    pub generation_number: i32,
    pub object: Option<Box<Value>>,
}

impl IndirectObjectOrReference {
    pub fn new(object_number: i32, generation_number: i32, object: Option<Box<Value>>) -> Self {
        IndirectObjectOrReference {
            object_number,
            generation_number,
            object,
        }
    }
}
