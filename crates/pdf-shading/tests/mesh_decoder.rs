//! Unit tests for private mesh decoding helpers.

use super::{MeshBitWidths, decode_sample};

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
