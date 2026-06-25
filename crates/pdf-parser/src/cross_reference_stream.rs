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
///
/// The stream data is decoded (filters such as FlateDecode and predictors
/// are applied) before reading the binary xref entries.
pub fn parse_xref_stream(
    stream: &StreamObject,
    objects: &dyn ObjectResolver,
) -> Result<CrossReferenceTable, ParserError> {
    let dict = stream.dictionary.as_ref();

    validate_stream_type(stream, objects)?;
    let size = dict.get_or_err("Size")?.try_number::<usize>(objects)?;
    let layout = XrefStreamLayout::from_dictionary(stream, objects)?;

    if layout.entry_width == 0 {
        return Ok(CrossReferenceTable::new(
            BTreeMap::new(),
            Trailer::new(stream.dictionary.clone(), 0),
        ));
    }

    let subsections = parse_subsections(stream, objects, size)?;
    let data = pdf_filter::filter::decode_with_resolver(stream, objects)?;
    let mut entries = BTreeMap::new();

    let mut decoder = XrefStreamEntryDecoder::new(&data, layout);
    for subsection in subsections {
        for object_offset in 0..subsection.count {
            let Some(entry) = decoder.next_entry() else {
                return Ok(CrossReferenceTable::new(
                    entries,
                    Trailer::new(stream.dictionary.clone(), 0),
                ));
            };

            let object_number = subsection.start.saturating_add(object_offset);
            entries.insert(object_number, entry);
        }
    }

    let trailer = Trailer::new(stream.dictionary.clone(), 0);
    Ok(CrossReferenceTable::new(entries, trailer))
}

/// Validates the optional `/Type` entry when it is present.
fn validate_stream_type(
    stream: &StreamObject,
    objects: &dyn ObjectResolver,
) -> Result<(), ParserError> {
    let Some(type_value) = stream.dictionary.get("Type") else {
        return Ok(());
    };

    let type_name = type_value.try_str(objects)?;
    if type_name.as_ref() == "XRef" {
        Ok(())
    } else {
        Err(ParserError::InvalidKeyword(
            "XRef".to_string(),
            type_name.into_owned(),
        ))
    }
}

/// Describes the byte layout of one decoded cross-reference stream entry.
#[derive(Clone, Copy)]
struct XrefStreamLayout {
    type_width: usize,
    second_field_width: usize,
    third_field_width: usize,
    entry_width: usize,
}

impl XrefStreamLayout {
    /// Reads the `/W` field widths from the stream dictionary.
    fn from_dictionary(
        stream: &StreamObject,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, ParserError> {
        let [type_width, second_field_width, third_field_width] = stream
            .dictionary
            .get_or_err("W")?
            .try_array_of::<usize, 3>(objects)?;

        Ok(Self {
            type_width,
            second_field_width,
            third_field_width,
            entry_width: type_width
                .saturating_add(second_field_width)
                .saturating_add(third_field_width),
        })
    }
}

/// A contiguous range of object numbers described by the `/Index` array.
#[derive(Clone, Copy)]
struct XrefSubsection {
    start: usize,
    count: usize,
}

/// Resolves `/Index`, defaulting to the full range declared by `/Size`.
fn parse_subsections(
    stream: &StreamObject,
    objects: &dyn ObjectResolver,
    size: usize,
) -> Result<Vec<XrefSubsection>, ParserError> {
    let index_values = stream.dictionary.get("Index").map_or_else(
        || Ok(vec![0, size]),
        |index_value| index_value.try_vec_of::<usize>(objects),
    )?;

    if index_values.len() % 2 != 0 {
        return Err(ParserError::InvalidKeyword(
            "Index array with even number of elements".to_string(),
            format!("Index array with {} elements", index_values.len()),
        ));
    }

    Ok(index_values
        .chunks_exact(2)
        .filter_map(|pair| match pair {
            [start, count] => Some(XrefSubsection {
                start: *start,
                count: *count,
            }),
            _ => None,
        })
        .collect())
}

/// Consumes fixed-width entries from decoded cross-reference stream data.
struct XrefStreamEntryDecoder<'data> {
    data: &'data [u8],
    layout: XrefStreamLayout,
    position: usize,
}

impl<'data> XrefStreamEntryDecoder<'data> {
    /// Creates a decoder at the beginning of the decoded stream data.
    fn new(data: &'data [u8], layout: XrefStreamLayout) -> Self {
        Self {
            data,
            layout,
            position: 0,
        }
    }

    /// Returns the next complete entry, or `None` when the data is truncated.
    fn next_entry(&mut self) -> Option<CrossReferenceEntry> {
        let end = self.position.saturating_add(self.layout.entry_width);
        let bytes = self.data.get(self.position..end)?;
        let entry = self.decode_entry(bytes)?;
        self.position = end;
        Some(entry)
    }

    /// Decodes one entry according to the `/W` field widths.
    fn decode_entry(&self, bytes: &[u8]) -> Option<CrossReferenceEntry> {
        let type_end = self.layout.type_width;
        let second_end = type_end.checked_add(self.layout.second_field_width)?;
        let third_end = second_end.checked_add(self.layout.third_field_width)?;

        let type_field = bytes.get(..type_end)?;
        let second_field = bytes.get(type_end..second_end)?;
        let third_field = bytes.get(second_end..third_end)?;

        let entry_type = if self.layout.type_width == 0 {
            1
        } else {
            read_field(type_field)
        };
        let second_value = read_field(second_field);
        let third_value = read_field(third_field);

        Some(match entry_type {
            0 => CrossReferenceEntry::new_free(second_value, third_value),
            1 => CrossReferenceEntry::new_normal(second_value, third_value),
            2 => CrossReferenceEntry::new_compressed(second_value, third_value),
            _ => CrossReferenceEntry::new_free(0, 0),
        })
    }
}

/// Reads an unsigned integer from big-endian bytes.
fn read_field(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .fold(0, |value, &byte| value.wrapping_shl(8) | usize::from(byte))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::as_conversions,
    clippy::arithmetic_side_effects
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

    #[test]
    fn test_parse_xref_stream_keeps_complete_entries_from_truncated_data() {
        let stream = build_xref_stream([1, 1, 1], None, 2, vec![1, 42, 0, 1, 84]);

        let table = parse_xref_stream(&stream, &PassthroughResolver).unwrap();

        assert_eq!(table.entries.len(), 1);
        assert_eq!(table.entries[&0].byte_offset(), Some(42));
    }

    #[test]
    fn test_parse_xref_stream_maps_unknown_entry_types_to_free_entries() {
        let stream = build_xref_stream([1, 1, 1], None, 1, vec![9, 42, 7]);

        let table = parse_xref_stream(&stream, &PassthroughResolver).unwrap();

        assert_eq!(table.entries[&0], CrossReferenceEntry::new_free(0, 0));
    }

    /// Builds a compressed xref stream with FlateDecode + PNG Up predictor,
    /// mimicking what real PDFs like linearized documents produce.
    fn build_compressed_xref_stream(
        w: [usize; 3],
        index: Option<Vec<usize>>,
        size: usize,
        raw_entries: Vec<u8>,
        columns: usize,
    ) -> StreamObject {
        use std::io::Write;

        // Apply PNG Up predictor (type 2): prefix each row with filter byte 2.
        // Row i: filter_byte=2, then delta from previous row.
        let row_bytes = columns;
        let mut predicted = Vec::new();
        let num_rows = raw_entries.len() / row_bytes;
        let mut prev_row: Vec<u8> = vec![0u8; row_bytes];
        for r in 0..num_rows {
            predicted.push(2u8); // PNG Up filter
            let row_start = r * row_bytes;
            for c in 0..row_bytes {
                let cur = raw_entries[row_start + c];
                let above = prev_row[c];
                predicted.push(cur.wrapping_sub(above));
            }
            prev_row = raw_entries[row_start..row_start + row_bytes].to_vec();
        }

        // Compress with zlib
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&predicted).unwrap();
        let compressed = encoder.finish().unwrap();

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
            "Filter".to_string(),
            ObjectVariant::Name(b"FlateDecode".to_vec()),
        );
        dict_map.insert(
            "DecodeParms".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(
                vec![
                    (
                        "Columns".to_string(),
                        ObjectVariant::Integer(columns as i64),
                    ),
                    ("Predictor".to_string(), ObjectVariant::Integer(12)),
                ]
                .into_iter()
                .collect(),
            ))),
        );
        dict_map.insert(
            "Length".to_string(),
            ObjectVariant::Integer(compressed.len() as i64),
        );

        StreamObject::new(0, 0, Box::new(Dictionary::new(dict_map)), compressed)
    }

    #[test]
    fn test_parse_xref_stream_flatedecode_with_predictor() {
        // Simulates /W[1 3 1] /Filter/FlateDecode /DecodeParms<</Columns 5/Predictor 12>>
        // which is exactly what ii.pdf uses.
        let mut raw_entries = Vec::new();
        // Entry 0: free, next=0, gen=255
        raw_entries.extend_from_slice(&[0, 0, 0, 0, 255]);
        // Entry 1: normal at offset 1000, gen=0
        raw_entries.extend_from_slice(&[1, 0, 3, 232, 0]); // 1000 = 0x03E8
        // Entry 2: normal at offset 2000, gen=0
        raw_entries.extend_from_slice(&[1, 0, 7, 208, 0]); // 2000 = 0x07D0
        // Entry 3: compressed in stream obj 1, index 0
        raw_entries.extend_from_slice(&[2, 0, 0, 1, 0]);

        let stream = build_compressed_xref_stream(
            [1, 3, 1],
            None,
            4,
            raw_entries,
            5, // columns = w1+w2+w3
        );
        let table = parse_xref_stream(&stream, &PassthroughResolver).unwrap();

        assert_eq!(table.entries.len(), 4);

        let e0 = &table.entries[&0];
        assert!(e0.is_free());

        let e1 = &table.entries[&1];
        assert_eq!(e1.byte_offset(), Some(1000));

        let e2 = &table.entries[&2];
        assert_eq!(e2.byte_offset(), Some(2000));

        let e3 = &table.entries[&3];
        assert!(e3.is_compressed());
        match &e3.entry_type {
            CrossReferenceEntryType::Compressed {
                object_stream_number,
                index_within_stream,
            } => {
                assert_eq!(*object_stream_number, 1);
                assert_eq!(*index_within_stream, 0);
            }
            _ => panic!("Expected compressed entry"),
        }
    }
}
