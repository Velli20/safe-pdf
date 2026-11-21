//! Unit tests for private mesh decoding helpers.

use pdf_utils::BitReader;

use super::{MeshBitReader, MeshDecoderError};

#[test]
fn mesh_reads_distinguish_clean_eof_from_truncation() {
    let mut empty = MeshBitReader(BitReader::new(&[]));
    assert_eq!(empty.read_field(8), Ok(None));

    let mut truncated = MeshBitReader(BitReader::new(&[0]));
    assert_eq!(
        truncated.read_field(9),
        Err(MeshDecoderError::TruncatedSample)
    );
    assert_eq!(truncated.0.pos(), 0);
}
