use pdf_object::{
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};
use pdf_parser::parser::PdfParser;

use crate::error::PdfReaderError;

/// An object extracted from a compressed PDF object stream.
#[derive(Debug, Clone)]
pub(crate) struct CompressedObject {
    /// The extracted object's indirect object number.
    pub(crate) number: usize,
    /// The extracted object value.
    pub(crate) value: ObjectVariant,
}

/// Locates one object within an object stream's decoded bytes.
struct ObjectStreamEntry {
    /// The object's indirect object number.
    number: usize,
    /// The object's byte offset relative to the stream's `/First` value.
    relative_offset: usize,
}

/// Parses an object stream (PDF 1.5+) and extracts all objects stored within it.
///
/// # Parameters
///
/// - `stream`: The object stream to parse.
/// - `objects`: Object resolver for parsing embedded objects.
///
/// # Returns
///
/// A vector of named compressed objects extracted from the stream.
pub(crate) fn read_object_stream(
    stream: &StreamObject,
    objects: &dyn ObjectResolver,
) -> Result<Vec<CompressedObject>, PdfReaderError> {
    let dict = stream.dictionary.as_ref();

    // /N: number of objects in this stream (required)
    let n = dict.required_number::<usize>(b"N", objects)?;

    // /First: byte offset of the first object data within the decoded stream (required)
    let first = dict.required_number::<usize>(b"First", objects)?;

    // Decode stream data (applies filters)
    let data = stream.raw_data();

    // Parse the header: N pairs of (object_number, relative_byte_offset)
    let mut header_parser = PdfParser::from(data);
    let mut object_entries = Vec::with_capacity(n);

    for _ in 0..n {
        let obj_num = header_parser.read_number::<usize>(true)?;
        let offset = header_parser.read_number::<usize>(true)?;
        object_entries.push(ObjectStreamEntry {
            number: obj_num,
            relative_offset: offset,
        });
    }

    // Parse each object from the data section
    let mut result = Vec::with_capacity(n);

    for entry in object_entries {
        let abs_offset = first.saturating_add(entry.relative_offset);
        let Some(slice) = data.get(abs_offset..) else {
            break;
        };

        let mut obj_parser = PdfParser::from(slice);
        let object = obj_parser.parse_object(objects)?;
        result.push(CompressedObject {
            number: entry.number,
            value: object,
        });
    }

    Ok(result)
}
