//! Unit tests for private mesh decoding helpers.

use pdf_utils::BitReader;

use super::{MeshBitWidths, decode_sample, read_mesh_bits, read_required_mesh_bits};

#[test]
fn decodes_sample_range_endpoints_and_midpoint() {
    assert_eq!(decode_sample(0, 8, -1.0, 1.0).expect("minimum"), -1.0);
    assert_eq!(decode_sample(255, 8, -1.0, 1.0).expect("maximum"), 1.0);
    let midpoint = decode_sample(128, 8, 0.0, 1.0).expect("midpoint");
    assert!((midpoint - (128.0 / 255.0)).abs() < f32::EPSILON);
}

#[test]
fn validates_pdf_mesh_bit_widths() {
    assert!(MeshBitWidths::new(12, 4, 2).is_ok());
    assert!(MeshBitWidths::new(3, 4, 2).is_err());
    assert!(MeshBitWidths::new(8, 32, 2).is_err());
    assert!(MeshBitWidths::new(8, 8, 1).is_err());
}

#[test]
fn mesh_reads_distinguish_clean_eof_from_truncation() {
    let mut empty = BitReader::new(&[]);
    assert!(matches!(read_mesh_bits(&mut empty, 8), Ok(None)));

    let mut truncated = BitReader::new(&[0]);
    assert!(read_mesh_bits(&mut truncated, 9).is_err());
    assert_eq!(truncated.pos(), 0);

    let mut required = BitReader::new(&[]);
    assert!(read_required_mesh_bits(&mut required, 8).is_err());
}

#[test]
fn mesh_reads_reject_invalid_widths() {
    let mut reader = BitReader::new(&[0]);

    assert!(read_mesh_bits(&mut reader, 0).is_err());
    assert!(read_mesh_bits(&mut reader, 33).is_err());
    assert_eq!(reader.pos(), 0);
}
