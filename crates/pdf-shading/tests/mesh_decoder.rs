//! Unit tests for private mesh decoding helpers.

use pdf_utils::BitReader;

use crate::error::PdfShadingError;

use super::{
    MeshBitWidths, MeshDecoderError, decode_sample, read_mesh_bits, read_required_mesh_bits,
};

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
    assert!(matches!(
        MeshBitWidths::new(3, 4, 2),
        Err(PdfShadingError::MeshDecoder(
            MeshDecoderError::InvalidBitsPerCoordinate { value: 3 }
        ))
    ));
    assert!(matches!(
        MeshBitWidths::new(8, 32, 2),
        Err(PdfShadingError::MeshDecoder(
            MeshDecoderError::InvalidBitsPerComponent { value: 32 }
        ))
    ));
    assert!(matches!(
        MeshBitWidths::new(8, 8, 1),
        Err(PdfShadingError::MeshDecoder(
            MeshDecoderError::InvalidBitsPerFlag { value: 1 }
        ))
    ));
}

#[test]
fn mesh_reads_distinguish_clean_eof_from_truncation() {
    let mut empty = BitReader::new(&[]);
    assert!(matches!(read_mesh_bits(&mut empty, 8), Ok(None)));

    let mut truncated = BitReader::new(&[0]);
    assert!(matches!(
        read_mesh_bits(&mut truncated, 9),
        Err(PdfShadingError::MeshDecoder(
            MeshDecoderError::TruncatedSample
        ))
    ));
    assert_eq!(truncated.pos(), 0);

    let mut required = BitReader::new(&[]);
    assert!(matches!(
        read_required_mesh_bits(&mut required, 8),
        Err(PdfShadingError::MeshDecoder(
            MeshDecoderError::UnexpectedEndOfStream
        ))
    ));
}

#[test]
fn mesh_reads_reject_invalid_widths() {
    let mut reader = BitReader::new(&[0]);

    assert!(matches!(
        read_mesh_bits(&mut reader, 0),
        Err(PdfShadingError::MeshDecoder(
            MeshDecoderError::InvalidBitFieldWidth { width: 0 }
        ))
    ));
    assert!(matches!(
        read_mesh_bits(&mut reader, 33),
        Err(PdfShadingError::MeshDecoder(
            MeshDecoderError::InvalidBitFieldWidth { width: 33 }
        ))
    ));
    assert_eq!(reader.pos(), 0);
}
