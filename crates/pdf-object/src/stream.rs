use std::rc::Rc;

use crate::dictionary::Dictionary;

/// Represents a PDF stream object.
///
/// A stream object, like a string object, is a sequence of bytes. However, PDF
/// can store large amounts of data in a stream that it would not be practical
/// to store in a string. Streams are used for objects such as images, page content descriptions,
/// and font definitions.
#[derive(Debug, PartialEq, Clone)]
pub struct StreamObject {
    /// The object number, identifying this stream as an indirect object.
    pub object_number: i32,
    /// The generation number, used for PDF incremental updates.
    pub generation_number: i32,
    /// The dictionary associated with this stream.
    pub dictionary: Rc<Dictionary>,
    /// The raw, uncompressed, byte data of the stream.
    pub data: Vec<u8>,
}

impl StreamObject {
    pub fn new(
        object_number: i32,
        generation_number: i32,
        dictionary: Rc<Dictionary>,
        data: Vec<u8>,
    ) -> Self {
        StreamObject {
            object_number,
            generation_number,
            dictionary,
            data,
        }
    }
}
