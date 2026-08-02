//! Unit tests for private mesh decoding helpers.

use pdf_utils::BitReader;

use crate::error::PdfShadingError;

use super::{MeshDecoderError, read_mesh_bits};

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
