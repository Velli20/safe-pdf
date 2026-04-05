use std::collections::BTreeMap;

use pdf_object::{
    cross_reference_table::{CrossReferenceEntry, CrossReferenceTable},
    object_resolver::ObjectResolver,
    stream::StreamObject,
    trailer::Trailer,
};

use crate::error::ParserError;

/// Parses a cross-reference stream (PDF 1.5+) into a `CrossReferenceTable`.
///
/// Cross-reference streams replace traditional xref tables in modern PDFs.
/// They encode the same information in a compact binary format stored as a
/// stream object with `/Type /XRef`.
pub fn parse_xref_stream(
    stream: &StreamObject,
    objects: &dyn ObjectResolver,
) -> Result<CrossReferenceTable, ParserError> {
    let dict = stream.dictionary.as_ref();

    // Validate /Type is /XRef (optional per some implementations, but expected)
    if let Some(type_val) = dict.get("Type") {
        let type_name = type_val.try_str(objects)?;
        if type_name.as_ref() != "XRef" {
            return Err(ParserError::InvalidKeyword(
                "XRef".to_string(),
                type_name.into_owned(),
            ));
        }
    }

    // Get the number of objects to read.
    let size = dict.get_or_err("Size")?.try_number::<usize>(objects)?;

    // Read the /W array which defines the field widths for each entry.
    let [w1, w2, w3] = dict.get_or_err("W")?.try_array_of::<usize, 3>(objects)?;

    let entry_size = w1.saturating_add(w2).saturating_add(w3);
    if entry_size == 0 {
        return Ok(CrossReferenceTable::new(
            BTreeMap::new(),
            Trailer::new(stream.dictionary.clone(), 0),
        ));
    }

    // Read the /Index array if present, which defines subsections of object numbers.
    let index_pairs = dict.get("Index").map_or_else(
        || Ok(vec![0, size]),
        |index_val| index_val.try_vec_of::<usize>(objects),
    )?;

    if index_pairs.len() % 2 != 0 {
        return Err(ParserError::InvalidKeyword(
            "Index array with even number of elements".to_string(),
            format!("Index array with {} elements", index_pairs.len()),
        ));
    }

    // Decode stream data (applies filters like FlateDecode)
    let data = stream.data().map_err(ParserError::ObjectError)?;

    let mut entries = BTreeMap::new();
    let mut pos: usize = 0;

    // Process each subsection defined by /Index pairs
    for pair in index_pairs.chunks(2) {
        let start = pair.first().copied().unwrap_or(0);
        let count = pair.get(1).copied().unwrap_or(0);

        for i in 0..count {
            if pos.saturating_add(entry_size) > data.len() {
                break;
            }

            // Read type field (default to 1 if w1 == 0)
            let entry_type = if w1 == 0 {
                1
            } else {
                read_field(data.get(pos..pos.saturating_add(w1)).unwrap_or(&[]))
            };
            let f2_start = pos.saturating_add(w1);
            let f3_start = f2_start.saturating_add(w2);
            let field2 = read_field(
                data.get(f2_start..f2_start.saturating_add(w2))
                    .unwrap_or(&[]),
            );
            let field3 = read_field(
                data.get(f3_start..f3_start.saturating_add(w3))
                    .unwrap_or(&[]),
            );

            let entry = match entry_type {
                0 => CrossReferenceEntry::new_free(field2, field3),
                1 => CrossReferenceEntry::new_normal(field2, field3),
                2 => CrossReferenceEntry::new_compressed(field2, field3),
                _ => CrossReferenceEntry::new_free(0, 0),
            };

            entries.insert(start.saturating_add(i), entry);
            pos = pos.saturating_add(entry_size);
        }
    }

    let trailer = Trailer::new(stream.dictionary.clone(), 0);
    Ok(CrossReferenceTable::new(entries, trailer))
}

/// Reads an unsigned integer from big-endian bytes.
fn read_field(bytes: &[u8]) -> usize {
    let mut value: usize = 0;
    for &b in bytes {
        value = (value << 8) | usize::from(b);
    }
    value
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::as_conversions
)]
mod tests {
    use pdf_object::{
        cross_reference_table::CrossReferenceEntryType, dictionary::Dictionary,
        object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };
    use std::collections::BTreeMap;

    use super::*;

    /// Helper to build a minimal xref stream for testing.
    fn build_xref_stream(
        w: [usize; 3],
        index: Option<Vec<usize>>,
        size: usize,
        raw_data: Vec<u8>,
    ) -> StreamObject {
        let mut dict_map = BTreeMap::new();
        dict_map.insert("Type".to_string(), ObjectVariant::Name(b"XRef".to_vec()));
        dict_map.insert("Size".to_string(), ObjectVariant::Integer(size as i64));
        dict_map.insert(
            "W".to_string(),
            ObjectVariant::Array(
                w.iter()
                    .map(|&v| ObjectVariant::Integer(v as i64))
                    .collect(),
            ),
        );
        if let Some(idx) = index {
            dict_map.insert(
                "Index".to_string(),
                ObjectVariant::Array(
                    idx.iter()
                        .map(|&v| ObjectVariant::Integer(v as i64))
                        .collect(),
                ),
            );
        }
        dict_map.insert(
            "Length".to_string(),
            ObjectVariant::Integer(raw_data.len() as i64),
        );

        StreamObject::new(0, 0, Box::new(Dictionary::new(dict_map)), raw_data)
    }

    #[test]
    fn test_parse_xref_stream_all_types() {
        // W = [1 2 1]: 1 byte type, 2 bytes field2, 1 byte field3
        let mut data = Vec::new();
        // Entry 0: free
        data.extend_from_slice(&[0, 0, 0, 255]);
        // Entry 1: normal at offset 256
        data.extend_from_slice(&[1, 1, 0, 0]);
        // Entry 2: compressed in stream 5, index 0
        data.extend_from_slice(&[2, 0, 5, 0]);
        // Entry 3: normal at offset 512
        data.extend_from_slice(&[1, 2, 0, 0]);

        let stream = build_xref_stream([1, 2, 1], None, 4, data);
        let table = parse_xref_stream(&stream, &PassthroughResolver).unwrap();

        assert_eq!(table.entries.len(), 4);

        let e0 = &table.entries[&0];
        assert!(e0.is_free());

        let e1 = &table.entries[&1];
        assert_eq!(e1.byte_offset(), Some(256));

        let e2 = &table.entries[&2];
        assert!(e2.is_compressed());
        match &e2.entry_type {
            CrossReferenceEntryType::Compressed {
                object_stream_number,
                index_within_stream,
            } => {
                assert_eq!(*object_stream_number, 5);
                assert_eq!(*index_within_stream, 0);
            }
            _ => panic!("Expected compressed entry"),
        }

        let e3 = &table.entries[&3];
        assert_eq!(e3.byte_offset(), Some(512));
    }

    #[test]
    fn test_parse_xref_stream_w1_zero_defaults_to_type_1() {
        // When w1=0, type defaults to 1 (normal)
        let mut data = Vec::new();
        data.extend_from_slice(&[0, 100]);
        data.extend_from_slice(&[0, 200]);

        let stream = build_xref_stream([0, 2, 0], None, 2, data);
        let table = parse_xref_stream(&stream, &PassthroughResolver).unwrap();

        assert_eq!(table.entries.len(), 2);
        assert_eq!(table.entries[&0].byte_offset(), Some(100));
        assert_eq!(table.entries[&1].byte_offset(), Some(200));
    }

    #[test]
    fn test_parse_xref_stream_with_index() {
        // /Index [5 2 10 1] — objects 5-6 and object 10
        let mut data = Vec::new();
        data.extend_from_slice(&[1, 50, 0]);
        data.extend_from_slice(&[1, 60, 0]);
        data.extend_from_slice(&[1, 100, 0]);

        let stream = build_xref_stream([1, 1, 1], Some(vec![5, 2, 10, 1]), 11, data);
        let table = parse_xref_stream(&stream, &PassthroughResolver).unwrap();

        assert_eq!(table.entries.len(), 3);
        assert_eq!(table.entries[&5].byte_offset(), Some(50));
        assert_eq!(table.entries[&6].byte_offset(), Some(60));
        assert_eq!(table.entries[&10].byte_offset(), Some(100));
        assert!(!table.entries.contains_key(&0));
        assert!(!table.entries.contains_key(&7));
    }
}
