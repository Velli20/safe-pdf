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
    /// Shared byte data of the stream.
    ///
    /// The bytes may still be encoded when the stream was created directly
    /// from PDF input and filter decoding has not yet succeeded. Cloning the
    /// [`Arc`] shares the allocation rather than copying the bytes.
    pub data: Arc<Vec<u8>>,
    /// Indicates whether the stream's declared filter chain has been applied to
    /// the current bytes.
    filters_applied: bool,
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
            filters_applied: true,
        }
    }

    /// Creates a new [`StreamObject`] whose declared filter chain has not been applied.
    pub fn new_encoded(
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
            filters_applied: false,
        }
    }

    /// Returns whether the stream's declared filter chain has been applied.
    pub fn filters_applied(&self) -> bool {
        self.filters_applied
    }

    /// Replaces the stream bytes with the result of applying its filter chain.
    pub fn set_filtered_data(&mut self, data: impl Into<Arc<Vec<u8>>>) {
        self.data = data.into();
        self.filters_applied = true;
    }

    /// Returns a reference to the stream bytes.
    ///
    /// Use [`Self::filters_applied`] when the distinction between encoded and
    /// decoded bytes matters to the caller.
    pub fn raw_data(&self) -> &[u8] {
        self.data.as_slice()
    }

    /// Returns shared ownership of the current stream bytes.
    ///
    /// Cloning the returned [`Arc`] does not copy the underlying byte buffer.
    pub fn shared_data(&self) -> Arc<Vec<u8>> {
        Arc::clone(&self.data)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::StreamObject;
    use crate::dictionary::Dictionary;
    use crate::object_variant::ObjectVariant;

    #[test]
    fn constructors_record_filter_state() {
        let decoded = StreamObject::new(
            1,
            0,
            Box::new(Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new())),
            vec![1],
        );
        let encoded = StreamObject::new_encoded(
            2,
            0,
            Box::new(Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new())),
            vec![2],
        );

        assert!(decoded.filters_applied());
        assert!(!encoded.filters_applied());
    }

    #[test]
    fn setting_filtered_data_updates_bytes_and_state() {
        let mut stream = StreamObject::new_encoded(
            1,
            0,
            Box::new(Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new())),
            vec![1],
        );

        stream.set_filtered_data(vec![2, 3]);

        assert!(stream.filters_applied());
        assert_eq!(stream.raw_data(), &[2, 3]);
    }
}
