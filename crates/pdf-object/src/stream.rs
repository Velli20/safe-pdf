use std::borrow::Cow;

use crate::{dictionary::Dictionary, error::ObjectError};

/// Represents a PDF stream object.
///
/// A stream object, like a string object, is a sequence of bytes. However, PDF
/// can store large amounts of data in a stream that it would not be practical
/// to store in a string. Streams are used for objects such as images, page
/// content descriptions, and font definitions.
///
/// Stream data is decoded (decompressed) at insertion time by
/// [`ObjectCollection`], so the [`data`](Self::data) field always contains the
/// fully decoded bytes.
#[derive(Debug, PartialEq, Clone)]
pub struct StreamObject {
    /// The object number, identifying this stream as an indirect object.
    pub object_number: usize,
    /// The generation number, used for PDF incremental updates.
    pub generation_number: usize,
    /// The dictionary associated with this stream.
    pub dictionary: Box<Dictionary>,
    /// The decoded (decompressed) byte data of the stream.
    ///
    /// Filter chains declared in the `/Filter` dictionary entry are applied
    /// when the stream is first inserted into the object collection, so this
    /// field always holds the final, usable bytes.
    pub data: Vec<u8>,
}

impl StreamObject {
    /// Creates a new [`StreamObject`] with already-decoded data.
    pub fn new(
        object_number: usize,
        generation_number: usize,
        dictionary: Box<Dictionary>,
        data: Vec<u8>,
    ) -> Self {
        StreamObject {
            object_number,
            generation_number,
            dictionary,
            data,
        }
    }

    /// Returns a reference to the stream bytes.
    ///
    /// Because stream data is decoded at insertion time, this always returns
    /// the fully decompressed content.
    pub fn raw_data(&self) -> &[u8] {
        &self.data
    }

    /// Returns the stream bytes as a borrowed [`Cow`].
    ///
    /// This method exists for API compatibility with callers that expect a
    /// `Result<Cow<'_, [u8]>, ObjectError>`. Since stream data is already
    /// decoded, it always succeeds.
    pub fn data(&self) -> Result<Cow<'_, [u8]>, ObjectError> {
        Ok(Cow::Borrowed(&self.data))
    }
}
