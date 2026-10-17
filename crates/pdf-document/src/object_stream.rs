use bytes::Bytes;
use pdf_object_reader::{
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};
use pdf_parser::parser::PdfParser;

use crate::error::PdfReaderError;

/// Locates one object within an object stream's decoded bytes.
pub(crate) struct ObjectStreamEntry {
    /// The object's indirect object number.
    pub(crate) number: usize,
    /// The object's byte offset relative to the stream's `/First` value.
    pub(crate) relative_offset: usize,
}

/// An object stream (PDF 1.5+) whose members are parsed on demand.
///
/// The stream owns a shared handle to its bytes, so it does not borrow the
/// object collection it was resolved from and members can be inserted while
/// the stream is being read.
pub(crate) struct ObjectStream {
    /// The stream's decoded bytes: the header followed by the member objects.
    data: Bytes,
    /// Byte offset of the first member object within `data` (`/First`).
    first: usize,
    /// Number of members stored in the stream (`/N`).
    count: usize,
}

impl ObjectStream {
    /// Reads the `/N` and `/First` entries of an object stream.
    ///
    /// # Parameters
    ///
    /// - `stream`: The object stream to read.
    /// - `objects`: Object resolver for indirect dictionary values.
    pub(crate) fn from_stream(
        stream: &StreamObject,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfReaderError> {
        Ok(Self {
            count: stream.dictionary.required_number::<usize>(b"N", objects)?,
            first: stream
                .dictionary
                .required_number::<usize>(b"First", objects)?,
            data: stream.shared_data(),
        })
    }

    /// Returns a lazy iterator over the header's `(object number, offset)` pairs, in stream order.
    pub(crate) fn entries(&self) -> ObjectStreamEntries<'_> {
        ObjectStreamEntries {
            parser: PdfParser::from(self.data.as_ref()),
            remaining: self.count,
        }
    }

    /// Parses the member object located by `entry`.
    ///
    /// Returns `Ok(None)` when the entry's offset lies past the end of the stream data.
    pub(crate) fn parse_member(
        &self,
        entry: &ObjectStreamEntry,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<ObjectVariant>, PdfReaderError> {
        let offset = self.first.saturating_add(entry.relative_offset);
        let Some(slice) = self.data.get(offset..) else {
            return Ok(None);
        };
        Ok(Some(PdfParser::from(slice).parse_object(objects)?))
    }
}

/// Iterator over an object stream's header entries.
///
/// Yields at most `/N` entries and stops after the first malformed pair.
pub(crate) struct ObjectStreamEntries<'data> {
    /// Parser positioned at the next header pair.
    parser: PdfParser<'data>,
    /// Number of header pairs not yet read.
    remaining: usize,
}

impl ObjectStreamEntries<'_> {
    /// Reads one `(object number, relative offset)` header pair.
    fn read_entry(&mut self) -> Result<ObjectStreamEntry, PdfReaderError> {
        Ok(ObjectStreamEntry {
            number: self.parser.read_number::<usize>(true)?,
            relative_offset: self.parser.read_number::<usize>(true)?,
        })
    }
}

impl Iterator for ObjectStreamEntries<'_> {
    type Item = Result<ObjectStreamEntry, PdfReaderError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.remaining = self.remaining.checked_sub(1)?;
        let entry = self.read_entry();
        if entry.is_err() {
            self.remaining = 0;
        }
        Some(entry)
    }
}
