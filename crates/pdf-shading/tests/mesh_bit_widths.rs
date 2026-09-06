//! Unit tests for mesh bit-width parsing and validation.

use pdf_object_reader::{
    dictionary::Dictionary, object_error::ObjectError, object_resolver::PassthroughResolver,
    object_variant::ObjectVariant,
};

use crate::{error::PdfShadingError, mesh_decoder::MeshDecoderError};

use super::MeshBitWidths;

fn mesh_widths_dictionary(coordinate: i64, component: i64, flag: i64) -> Dictionary {
    Dictionary::from_entries([
        (b"BitsPerCoordinate", ObjectVariant::Integer(coordinate)),
        (b"BitsPerComponent", ObjectVariant::Integer(component)),
        (b"BitsPerFlag", ObjectVariant::Integer(flag)),
    ])
}

#[test]
fn validates_pdf_mesh_bit_widths() {
    let valid = mesh_widths_dictionary(12, 4, 2);
    let widths = MeshBitWidths::from_dictionary(&valid, &PassthroughResolver)
        .expect("valid widths should parse");
    assert_eq!(widths.coordinate(), 12);
    assert_eq!(widths.component(), 4);
    assert_eq!(widths.flag(), 2);

    let invalid_coordinate = mesh_widths_dictionary(3, 4, 2);
    assert!(matches!(
        MeshBitWidths::from_dictionary(&invalid_coordinate, &PassthroughResolver),
        Err(PdfShadingError::MeshDecoder(
            MeshDecoderError::InvalidBitsPerCoordinate { value: 3 }
        ))
    ));

    let invalid_component = mesh_widths_dictionary(8, 32, 2);
    assert!(matches!(
        MeshBitWidths::from_dictionary(&invalid_component, &PassthroughResolver),
        Err(PdfShadingError::MeshDecoder(
            MeshDecoderError::InvalidBitsPerComponent { value: 32 }
        ))
    ));

    let invalid_flag = mesh_widths_dictionary(8, 8, 1);
    assert!(matches!(
        MeshBitWidths::from_dictionary(&invalid_flag, &PassthroughResolver),
        Err(PdfShadingError::MeshDecoder(
            MeshDecoderError::InvalidBitsPerFlag { value: 1 }
        ))
    ));
}

#[test]
fn mesh_bit_widths_require_all_dictionary_entries() {
    let mut dictionary = mesh_widths_dictionary(8, 8, 2);
    dictionary.take(b"BitsPerFlag");

    assert!(matches!(
        MeshBitWidths::from_dictionary(&dictionary, &PassthroughResolver),
        Err(PdfShadingError::Object(ObjectError::MissingRequiredKey { ref key }))
            if key == "BitsPerFlag"
    ));
}
