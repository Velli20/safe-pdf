//! Unit tests for mesh bit-width parsing and validation.

use pdf_object_reader::{
    EmptySource, ObjectReadError, ObjectReader, ReadResult, dictionary::Dictionary,
    object_variant::ObjectVariant,
};

use crate::{error::PdfShadingError, mesh_decoder::MeshDecoderError};

use super::MeshBitWidths;

fn read_widths(dictionary: Dictionary) -> ReadResult<MeshBitWidths> {
    ObjectReader::new(EmptySource).read(&ObjectVariant::Dictionary(dictionary))
}

fn decoder_error(result: ReadResult<MeshBitWidths>) -> Option<MeshDecoderError> {
    match result {
        Err(ObjectReadError::Decode { source, .. }) => match source.downcast_ref() {
            Some(PdfShadingError::MeshDecoder(error)) => Some(error.clone()),
            _ => None,
        },
        _ => None,
    }
}

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
    let widths = read_widths(valid).expect("valid widths should parse");
    assert_eq!(widths.coordinate(), 12);
    assert_eq!(widths.component(), 4);
    assert_eq!(widths.flag(), 2);

    let invalid_coordinate = mesh_widths_dictionary(3, 4, 2);
    assert_eq!(
        decoder_error(read_widths(invalid_coordinate)),
        Some(MeshDecoderError::InvalidBitsPerCoordinate { value: 3 })
    );

    let invalid_component = mesh_widths_dictionary(8, 32, 2);
    assert_eq!(
        decoder_error(read_widths(invalid_component)),
        Some(MeshDecoderError::InvalidBitsPerComponent { value: 32 })
    );

    let invalid_flag = mesh_widths_dictionary(8, 8, 1);
    assert_eq!(
        decoder_error(read_widths(invalid_flag)),
        Some(MeshDecoderError::InvalidBitsPerFlag { value: 1 })
    );
}

#[test]
fn mesh_bit_widths_require_all_dictionary_entries() {
    let mut dictionary = mesh_widths_dictionary(8, 8, 2);
    dictionary.take(b"BitsPerFlag");

    assert!(matches!(
        read_widths(dictionary),
        Err(ObjectReadError::At { source, .. })
            if matches!(*source, ObjectReadError::MissingDictionaryKey { ref key } if key.as_bytes() == b"BitsPerFlag")
    ));
}
