use std::collections::BTreeMap;
use std::sync::Arc;

use pdf_filter::filter::DecodedStream;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
    cross_reference_table::{CrossReferenceEntryType, CrossReferenceTable},
    trailer::Trailer,
};

/// A cross-reference stream (PDF 1.5+) decoded into its table of entries and its trailer.
///
/// Cross-reference streams replace traditional xref tables in modern PDFs. They encode
/// the same information in a compact binary format stored as a stream object with
/// `/Type /XRef`. The stream data is filter decoded before its entries are read.
pub(crate) struct XrefStream(pub(crate) CrossReferenceTable);

impl FromPdfObject for XrefStream {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut stream = context.stream()?;
        let mut dictionary = stream.dictionary();
        if let Some(type_name) = dictionary.optional::<Arc<[u8]>>(b"Type")?
            && *type_name != *b"XRef"
        {
            return Err(ObjectReadError::Decode {
                target: "cross-reference stream",
                source: format!(
                    "expected /Type XRef, found {}",
                    String::from_utf8_lossy(&type_name)
                )
                .into(),
            });
        }
        let layout: XrefStreamLayout = dictionary.required(b"W")?;
        let size: usize = dictionary.required(b"Size")?;
        let index: Vec<usize> = dictionary
            .optional(b"Index")?
            .unwrap_or_else(|| vec![0, size]);
        let trailer = Trailer::new(Box::new(stream.stream().dictionary.clone()), None);

        if layout.entry_width == 0 {
            return Ok(Self(CrossReferenceTable::new(BTreeMap::new(), trailer)));
        }

        let (subsections, []) = index.as_chunks::<2>() else {
            return Err(ObjectReadError::Decode {
                target: "cross-reference stream",
                source: format!(
                    "expected an /Index array with an even number of elements, found {}",
                    index.len()
                )
                .into(),
            });
        };
        let DecodedStream(data) = stream.read(object)?;
        let mut decoder = XrefStreamEntryDecoder::new(&data, layout);
        let mut entries = BTreeMap::new();
        for &[start, count] in subsections {
            for object_offset in 0..count {
                let Some(entry) = decoder.next_entry() else {
                    return Ok(Self(CrossReferenceTable::new(entries, trailer)));
                };
                entries.insert(start.saturating_add(object_offset), entry);
            }
        }

        Ok(Self(CrossReferenceTable::new(entries, trailer)))
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

/// Reads the `/W` field widths.
impl FromPdfObject for XrefStreamLayout {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let [type_width, second_field_width, third_field_width] =
            context.array()?.read_array::<usize, 3>()?;
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
    fn next_entry(&mut self) -> Option<CrossReferenceEntryType> {
        let end = self.position.saturating_add(self.layout.entry_width);
        let bytes = self.data.get(self.position..end)?;
        let entry = self.decode_entry(bytes)?;
        self.position = end;
        Some(entry)
    }

    /// Decodes one entry according to the `/W` field widths.
    fn decode_entry(&self, bytes: &[u8]) -> Option<CrossReferenceEntryType> {
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
            0 => CrossReferenceEntryType::new_free(second_value, third_value),
            1 => CrossReferenceEntryType::new_normal(second_value, third_value),
            2 => CrossReferenceEntryType::new_compressed(second_value),
            _ => CrossReferenceEntryType::new_free(0, 0),
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
    use pdf_object_reader::{
        EmptySource, ObjectReader, cross_reference_table::CrossReferenceEntryType,
        dictionary::Dictionary, object_variant::ObjectVariant, stream::StreamObject,
    };
    use std::collections::BTreeMap;

    use super::*;

    fn parse_xref_stream(stream: StreamObject) -> ReadResult<CrossReferenceTable> {
        let XrefStream(table) =
            ObjectReader::new(EmptySource).read(&ObjectVariant::Stream(stream))?;
        Ok(table)
    }

    /// Helper to build a minimal xref stream for testing.
    fn build_xref_stream(
        w: [usize; 3],
        index: Option<Vec<usize>>,
        size: usize,
        raw_data: Vec<u8>,
    ) -> StreamObject {
        let mut dict_map = BTreeMap::new();
        dict_map.insert(
            Vec::from(b"Type"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"XRef",
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );
        dict_map.insert(Vec::from(b"Size"), ObjectVariant::Integer(size as i64));
        dict_map.insert(
            Vec::from(b"W"),
            ObjectVariant::Array(
                w.iter()
                    .map(|&v| ObjectVariant::Integer(v as i64))
                    .collect(),
            ),
        );
        if let Some(idx) = index {
            dict_map.insert(
                Vec::from(b"Index"),
                ObjectVariant::Array(
                    idx.iter()
                        .map(|&v| ObjectVariant::Integer(v as i64))
                        .collect(),
                ),
            );
        }
        dict_map.insert(
            Vec::from(b"Length"),
            ObjectVariant::Integer(raw_data.len() as i64),
        );

        StreamObject::new_encoded(0, 0, Dictionary::from_iter(dict_map), raw_data)
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
        let table = parse_xref_stream(stream).unwrap();

        assert_eq!(table.entries.len(), 4);

        let e0 = &table.entries[&0];
        assert!(e0.is_free());

        let e1 = &table.entries[&1];
        assert_eq!(e1.byte_offset(), Some(256));

        let e2 = &table.entries[&2];
        assert!(e2.is_compressed());
        match e2 {
            CrossReferenceEntryType::Compressed {
                object_stream_number,
            } => {
                assert_eq!(*object_stream_number, 5);
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
        let table = parse_xref_stream(stream).unwrap();

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
        let table = parse_xref_stream(stream).unwrap();

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

        let table = parse_xref_stream(stream).unwrap();

        assert_eq!(table.entries.len(), 1);
        assert_eq!(table.entries[&0].byte_offset(), Some(42));
    }

    #[test]
    fn test_parse_xref_stream_maps_unknown_entry_types_to_free_entries() {
        let stream = build_xref_stream([1, 1, 1], None, 1, vec![9, 42, 7]);

        let table = parse_xref_stream(stream).unwrap();

        assert_eq!(table.entries[&0], CrossReferenceEntryType::new_free(0, 0));
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
        dict_map.insert(
            Vec::from(b"Type"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"XRef",
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );
        dict_map.insert(Vec::from(b"Size"), ObjectVariant::Integer(size as i64));
        dict_map.insert(
            Vec::from(b"W"),
            ObjectVariant::Array(
                w.iter()
                    .map(|&v| ObjectVariant::Integer(v as i64))
                    .collect(),
            ),
        );
        if let Some(idx) = index {
            dict_map.insert(
                Vec::from(b"Index"),
                ObjectVariant::Array(
                    idx.iter()
                        .map(|&v| ObjectVariant::Integer(v as i64))
                        .collect(),
                ),
            );
        }
        dict_map.insert(
            Vec::from(b"Filter"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"FlateDecode",
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        );
        dict_map.insert(
            Vec::from(b"DecodeParms"),
            ObjectVariant::Dictionary(Dictionary::from_iter(vec![
                (
                    Vec::from(b"Columns"),
                    ObjectVariant::Integer(columns as i64),
                ),
                (Vec::from(b"Predictor"), ObjectVariant::Integer(12)),
            ])),
        );
        dict_map.insert(
            Vec::from(b"Length"),
            ObjectVariant::Integer(compressed.len() as i64),
        );

        StreamObject::new_encoded(0, 0, Dictionary::from_iter(dict_map), compressed)
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
        let table = parse_xref_stream(stream).unwrap();

        assert_eq!(table.entries.len(), 4);

        let e0 = &table.entries[&0];
        assert!(e0.is_free());

        let e1 = &table.entries[&1];
        assert_eq!(e1.byte_offset(), Some(1000));

        let e2 = &table.entries[&2];
        assert_eq!(e2.byte_offset(), Some(2000));

        let e3 = &table.entries[&3];
        assert!(e3.is_compressed());
        match e3 {
            CrossReferenceEntryType::Compressed {
                object_stream_number,
            } => {
                assert_eq!(*object_stream_number, 1);
            }
            _ => panic!("Expected compressed entry"),
        }
    }
}
