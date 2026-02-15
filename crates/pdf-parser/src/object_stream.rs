use pdf_object::{
    object_resolver::ObjectResolver, object_variant::ObjectVariant, stream::StreamObject,
};

use crate::{error::ParserError, parser::PdfParser};

/// Parses an object stream (PDF 1.5+) and extracts all objects stored within it.
///
/// An object stream is a stream object that contains a sequence of PDF objects
/// compressed together. This allows multiple small objects to share a single
/// compression context, significantly reducing file size.
///
/// Per the PDF spec:
/// - Objects in object streams always have generation number 0
/// - Objects in object streams cannot themselves be streams
/// - The `/N` entry gives the number of objects
/// - The `/First` entry gives the byte offset within the decoded data to the first object
///
/// # Parameters
///
/// - `stream`: The object stream to parse.
/// - `objects`: Object resolver for parsing embedded objects.
///
/// # Returns
///
/// A vector of `(object_number, ObjectVariant)` pairs for each object in the stream.
pub fn parse_object_stream(
    stream: &StreamObject,
    objects: &dyn ObjectResolver,
) -> Result<Vec<(usize, ObjectVariant)>, ParserError> {
    let dict = stream.dictionary.as_ref();

    // /N: number of objects in this stream (required)
    let n: usize = dict.get_or_err("N")?.try_number(objects)?;

    // /First: byte offset of the first object data within the decoded stream (required)
    let first: usize = dict.get_or_err("First")?.try_number(objects)?;

    // Decode stream data (applies filters)
    let data = stream.data().map_err(ParserError::ObjectError)?;

    // Parse the header: N pairs of (object_number, relative_byte_offset)
    let mut header_parser = PdfParser::from(data.as_ref());
    let mut obj_entries = Vec::with_capacity(n);

    for _ in 0..n {
        let obj_num: usize = header_parser.read_number(true)?;
        let offset: usize = header_parser.read_number(true)?;
        obj_entries.push((obj_num, offset));
    }

    // Parse each object from the data section
    let mut result = Vec::with_capacity(n);

    for &(obj_num, rel_offset) in &obj_entries {
        let abs_offset = first.saturating_add(rel_offset);
        let Some(slice) = data.get(abs_offset..) else {
            break;
        };

        let mut obj_parser = PdfParser::from(slice);
        let object = obj_parser.parse_object(objects)?;
        result.push((obj_num, object));
    }

    Ok(result)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::as_conversions, clippy::indexing_slicing)]
mod tests {
    use pdf_object::{dictionary::Dictionary, object_resolver::UnimplementedResolver};
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn test_parse_object_stream_basic() {
        // Build an object stream with 2 objects:
        //   Object 10: integer 42
        //   Object 11: boolean true
        //
        // Header: "10 0 11 3 "
        //   obj 10 at relative offset 0 => "/First + 0"
        //   obj 11 at relative offset 3 => "/First + 3"
        // Data after First:
        //   "42 true"

        let stream_content = b"10 0 11 3 42 true";
        let first = 10; // "10 0 11 3 " is 10 bytes

        let mut dict_map = BTreeMap::new();
        dict_map.insert("Type".to_string(), ObjectVariant::Name("ObjStm".to_string()));
        dict_map.insert("N".to_string(), ObjectVariant::Integer(2));
        dict_map.insert("First".to_string(), ObjectVariant::Integer(first as i64));
        dict_map.insert(
            "Length".to_string(),
            ObjectVariant::Integer(stream_content.len() as i64),
        );

        let stream = StreamObject::new(
            99, // object number of the stream itself
            0,
            Box::new(Dictionary::new(dict_map)),
            stream_content.to_vec(),
            None,
        );

        let result = parse_object_stream(&stream, &UnimplementedResolver).unwrap();
        assert_eq!(result.len(), 2);

        assert_eq!(result[0].0, 10);
        assert_eq!(result[0].1, ObjectVariant::Integer(42));

        assert_eq!(result[1].0, 11);
        assert_eq!(result[1].1, ObjectVariant::Boolean(true));
    }

    #[test]
    fn test_parse_object_stream_with_dict() {
        // Object 5: a dictionary << /Key /Value >>
        // Header: "5 0 "
        // Data: "<< /Key /Value >>"
        let stream_content = b"5 0 << /Key /Value >>";
        let first = 4; // "5 0 " is 4 bytes

        let mut dict_map = BTreeMap::new();
        dict_map.insert("Type".to_string(), ObjectVariant::Name("ObjStm".to_string()));
        dict_map.insert("N".to_string(), ObjectVariant::Integer(1));
        dict_map.insert("First".to_string(), ObjectVariant::Integer(first as i64));
        dict_map.insert(
            "Length".to_string(),
            ObjectVariant::Integer(stream_content.len() as i64),
        );

        let stream = StreamObject::new(
            99,
            0,
            Box::new(Dictionary::new(dict_map)),
            stream_content.to_vec(),
            None,
        );

        let result = parse_object_stream(&stream, &UnimplementedResolver).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, 5);
        match &result[0].1 {
            ObjectVariant::Dictionary(d) => {
                assert_eq!(
                    d.get("Key"),
                    Some(&ObjectVariant::Name("Value".to_string()))
                );
            }
            other => panic!("Expected dictionary, got {:?}", other),
        }
    }
}
