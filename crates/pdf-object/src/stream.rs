use std::sync::Arc;

use crate::dictionary::Dictionary;

/// Represents a PDF stream object.
///
/// A stream object, like a string object, is a sequence of bytes. However, PDF
/// can store large amounts of data in a stream that it would not be practical
/// to store in a string. Streams are used for objects such as images, page
/// content descriptions, and font definitions.
#[derive(Debug, PartialEq, Clone)]
pub struct StreamObject {
    /// The object number, identifying this stream as an indirect object.
    pub object_number: usize,
    /// The generation number, used for PDF incremental updates.
    pub generation_number: usize,
    /// The dictionary associated with this stream.
    pub dictionary: Box<Dictionary>,
    /// Shared decoded (decompressed) byte data of the stream.
    ///
    /// Filter chains declared in the `/Filter` dictionary entry are applied
    /// when the stream is first inserted into the object collection, so this
    /// field always holds the final, usable bytes. Cloning the [`Arc`] shares
    /// the allocation rather than copying the bytes.
    pub data: Arc<Vec<u8>>,
}

impl StreamObject {
    /// Creates a new [`StreamObject`] with already-decoded data.
    pub fn new(
        object_number: usize,
        generation_number: usize,
        dictionary: Box<Dictionary>,
        data: impl Into<Arc<Vec<u8>>>,
    ) -> Self {
        StreamObject {
            object_number,
            generation_number,
            dictionary,
            data: data.into(),
        }
    }

    /// Returns a reference to the stream bytes.
    ///
    /// Because stream data is decoded at insertion time, this always returns
    /// the fully decompressed content.
    pub fn raw_data(&self) -> &[u8] {
        self.data.as_slice()
    }

    /// Returns shared ownership of the decoded stream bytes.
    ///
    /// Cloning the returned [`Arc`] does not copy the underlying byte buffer.
    pub fn shared_data(&self) -> Arc<Vec<u8>> {
        Arc::clone(&self.data)
    }
}
