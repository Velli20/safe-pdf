//! Unit tests for the private packed-sample reader.

use super::MeshSampleReader;

#[test]
fn reads_fields_across_byte_boundaries() {
    let mut reader = MeshSampleReader::new(&[0b1011_0010, 0b0110_0000]);

    assert_eq!(reader.read_bits(3).expect("first field"), Some(0b101));
    assert_eq!(
        reader.read_bits(7).expect("cross-byte field"),
        Some(0b100_1001)
    );
    assert_eq!(reader.read_bits(6).expect("last field"), Some(0b10_0000));
    assert_eq!(reader.read_bits(1).expect("end of stream"), None);
}

#[test]
fn aligns_to_the_next_byte() {
    let mut reader = MeshSampleReader::new(&[0b1110_0000, 0b1010_0101]);

    assert_eq!(reader.read_bits(3).expect("prefix"), Some(0b111));
    reader.align_to_byte();
    assert_eq!(
        reader.read_bits(8).expect("aligned byte"),
        Some(0b1010_0101)
    );
}

#[test]
fn rejects_invalid_widths_and_truncated_fields() {
    let mut reader = MeshSampleReader::new(&[0]);
    assert!(reader.read_bits(0).is_err());
    assert!(reader.read_bits(33).is_err());
    assert!(reader.read_bits(9).is_err());
}
