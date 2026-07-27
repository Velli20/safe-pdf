//! End-to-end regression tests for shading parsing.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;

use pdf_object::{
    dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};
use pdf_shading::{
    error::PdfShadingError,
    model::{MeshTriangle, Shading},
};

fn integer(value: i64) -> ObjectVariant {
    ObjectVariant::Integer(value)
}

fn number_array(values: &[f32]) -> ObjectVariant {
    ObjectVariant::Array(
        values
            .iter()
            .map(|value| ObjectVariant::Real(f64::from(*value)))
            .collect(),
    )
}

fn type_2_rgb_function() -> ObjectVariant {
    ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([
        ("FunctionType".to_string(), integer(2)),
        ("Domain".to_string(), number_array(&[0.0, 1.0])),
        ("C0".to_string(), number_array(&[1.0, 0.0, 0.0])),
        ("C1".to_string(), number_array(&[0.0, 0.0, 1.0])),
        ("N".to_string(), ObjectVariant::Real(1.0)),
    ]))))
}

fn free_form_stream(
    data: Vec<u8>,
    bits_per_flag: i64,
    component_pairs: usize,
    function: Option<ObjectVariant>,
) -> ObjectVariant {
    let mut decode = vec![0.0, 15.0, 0.0, 15.0];
    for _ in 0..component_pairs {
        decode.extend([0.0, 1.0]);
    }
    let mut entries = mesh_entries(4, 4, 4, bits_per_flag, decode);
    if let Some(function) = function {
        entries.insert("Function".to_string(), function);
    }

    shading_stream(entries, data)
}

fn patch_stream(
    shading_type: i64,
    data: Vec<u8>,
    widths: [i64; 3],
    decode: Vec<f32>,
    function: Option<ObjectVariant>,
) -> ObjectVariant {
    let [coordinate, component, flag] = widths;
    let mut entries = mesh_entries(shading_type, coordinate, component, flag, decode);
    if let Some(function) = function {
        entries.insert("Function".to_string(), function);
    }
    shading_stream(entries, data)
}

fn mesh_entries(
    shading_type: i64,
    bits_per_coordinate: i64,
    bits_per_component: i64,
    bits_per_flag: i64,
    decode: Vec<f32>,
) -> BTreeMap<String, ObjectVariant> {
    BTreeMap::from([
        ("ShadingType".to_string(), integer(shading_type)),
        (
            "ColorSpace".to_string(),
            ObjectVariant::Name(b"DeviceRGB".to_vec()),
        ),
        (
            "BitsPerCoordinate".to_string(),
            integer(bits_per_coordinate),
        ),
        ("BitsPerComponent".to_string(), integer(bits_per_component)),
        ("BitsPerFlag".to_string(), integer(bits_per_flag)),
        ("Decode".to_string(), number_array(&decode)),
    ])
}

fn shading_stream(entries: BTreeMap<String, ObjectVariant>, data: Vec<u8>) -> ObjectVariant {
    ObjectVariant::Stream(StreamObject::new(
        1,
        0,
        Box::new(Dictionary::new(entries)),
        data,
    ))
}

fn push_field(bits: &mut Vec<bool>, value: u32, width: usize) {
    for shift in (0..width).rev() {
        bits.push(value & (1_u32 << shift) != 0);
    }
}

fn encode_vertex(flag: u32, point: [u32; 2], components: &[u32], bits_per_flag: usize) -> Vec<u8> {
    encode_vertex_with_widths(flag, point, components, [4, 4, bits_per_flag])
}

fn encode_vertex_with_widths(
    flag: u32,
    point: [u32; 2],
    components: &[u32],
    widths: [usize; 3],
) -> Vec<u8> {
    let [coordinate_width, component_width, flag_width] = widths;
    let mut bits = Vec::new();
    push_field(&mut bits, flag, flag_width);
    push_field(&mut bits, point[0], coordinate_width);
    push_field(&mut bits, point[1], coordinate_width);
    for component in components {
        push_field(&mut bits, *component, component_width);
    }

    bits.chunks(8)
        .map(|chunk| {
            let value = chunk
                .iter()
                .fold(0_u8, |value, bit| (value << 1) | u8::from(*bit));
            value << (8_usize.saturating_sub(chunk.len()))
        })
        .collect()
}

fn append_vertex(
    data: &mut Vec<u8>,
    flag: u32,
    point: [u32; 2],
    components: &[u32],
    bits_per_flag: usize,
) {
    data.extend(encode_vertex(flag, point, components, bits_per_flag));
}

fn parsed_triangles(object: &ObjectVariant) -> Vec<MeshTriangle> {
    let shading = Shading::from_dictionary(object, &PassthroughResolver)
        .expect("free-form triangle mesh should parse");
    match shading {
        Shading::FreeFormTriangleMesh { triangles, .. } => triangles,
        _ => Vec::new(),
    }
}

fn append_patch_record(data: &mut Vec<u8>, flag: u8, point_count: usize, color_count: usize) {
    data.push(flag);
    for index in 0..point_count {
        data.extend([
            u8::try_from(index).expect("test point index fits in u8"),
            u8::try_from(index.saturating_add(1)).expect("test point index fits in u8"),
        ]);
    }
    for index in 0..color_count {
        let component =
            u8::try_from(index.saturating_mul(32)).expect("test color component fits in u8");
        data.extend([component, component, component]);
    }
}

fn patch_decode() -> Vec<f32> {
    vec![0.0, 255.0, 0.0, 255.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]
}

#[test]
fn parses_byte_aligned_free_form_triangle_vertices() {
    let mut data = Vec::new();
    append_vertex(&mut data, 0, [0, 0], &[15, 0, 0], 2);
    append_vertex(&mut data, 3, [15, 0], &[0, 15, 0], 2);
    append_vertex(&mut data, 3, [0, 15], &[0, 0, 15], 2);

    let triangles = parsed_triangles(&free_form_stream(data, 2, 3, None));

    assert_eq!(triangles.len(), 1);
    let triangle = triangles.first().expect("triangle should exist");
    let [red, green, blue] = triangle.vertices;
    assert_eq!(red.point, pdf_graphics::point::Point::new(0.0, 0.0));
    assert_eq!(green.point, pdf_graphics::point::Point::new(15.0, 0.0));
    assert_eq!(blue.point, pdf_graphics::point::Point::new(0.0, 15.0));
    assert_eq!(
        red.color,
        pdf_graphics::color::Color::from_rgb(1.0, 0.0, 0.0)
    );
    assert_eq!(
        green.color,
        pdf_graphics::color::Color::from_rgb(0.0, 1.0, 0.0)
    );
    assert_eq!(
        blue.color,
        pdf_graphics::color::Color::from_rgb(0.0, 0.0, 1.0)
    );
}

#[test]
fn parses_32_bit_mesh_coordinates() {
    let widths = [32, 8, 2];
    let mut data = Vec::new();
    data.extend(encode_vertex_with_widths(0, [0, 0], &[255, 0, 0], widths));
    data.extend(encode_vertex_with_widths(
        0,
        [u32::MAX, 0],
        &[0, 255, 0],
        widths,
    ));
    data.extend(encode_vertex_with_widths(
        0,
        [0, u32::MAX],
        &[0, 0, 255],
        widths,
    ));
    let object = shading_stream(
        mesh_entries(
            4,
            32,
            8,
            2,
            vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        ),
        data,
    );

    let triangles = parsed_triangles(&object);
    let triangle = triangles.first().expect("triangle should exist");

    assert_eq!(
        triangle.vertices[1].point,
        pdf_graphics::point::Point::new(1.0, 0.0)
    );
    assert_eq!(
        triangle.vertices[2].point,
        pdf_graphics::point::Point::new(0.0, 1.0)
    );
}

#[test]
fn reconstructs_free_form_continuations_from_low_flag_bits() {
    let mut data = Vec::new();
    append_vertex(&mut data, 0, [0, 0], &[15, 0, 0], 4);
    append_vertex(&mut data, 0, [15, 0], &[0, 15, 0], 4);
    append_vertex(&mut data, 0, [0, 15], &[0, 0, 15], 4);
    append_vertex(&mut data, 0b1101, [15, 15], &[15, 15, 15], 4);
    append_vertex(&mut data, 0b1110, [8, 8], &[0, 0, 0], 4);

    let triangles = parsed_triangles(&free_form_stream(data, 4, 3, None));
    let first = triangles.first().expect("first triangle should exist");
    let second = triangles.get(1).expect("second triangle should exist");
    let third = triangles.get(2).expect("third triangle should exist");

    assert_eq!(
        second.vertices,
        [first.vertices[1], first.vertices[2], second.vertices[2]]
    );
    assert_eq!(
        third.vertices,
        [second.vertices[0], second.vertices[2], third.vertices[2]]
    );
}

#[test]
fn applies_optional_function_to_vertex_parameter() {
    let mut data = Vec::new();
    append_vertex(&mut data, 0, [0, 0], &[0], 2);
    append_vertex(&mut data, 0, [15, 0], &[15], 2);
    append_vertex(&mut data, 0, [0, 15], &[0], 2);

    let triangles = parsed_triangles(&free_form_stream(data, 2, 1, Some(type_2_rgb_function())));
    let triangle = triangles.first().expect("triangle should exist");

    assert_eq!(
        triangle.vertices[0].color,
        pdf_graphics::color::Color::from_rgb(1.0, 0.0, 0.0)
    );
    assert_eq!(
        triangle.vertices[1].color,
        pdf_graphics::color::Color::from_rgb(0.0, 0.0, 1.0)
    );
}

#[test]
fn rejects_invalid_or_incomplete_triangle_streams() {
    let error_cases = [
        free_form_stream(Vec::new(), 2, 3, None),
        free_form_stream(encode_vertex(1, [0, 0], &[15, 0, 0], 2), 2, 3, None),
        free_form_stream(encode_vertex(3, [0, 0], &[15, 0, 0], 2), 2, 3, None),
        free_form_stream(encode_vertex(0, [0, 0], &[15, 0, 0], 2), 2, 3, None),
        free_form_stream(vec![0], 2, 3, None),
    ];

    for object in error_cases {
        assert!(matches!(
            Shading::from_dictionary(&object, &PassthroughResolver),
            Err(PdfShadingError::InvalidShadingMeshData { .. })
        ));
    }
}

#[test]
fn parses_coons_and_tensor_patch_streams() {
    let cases = [(6, 12), (7, 16)];

    for (shading_type, point_count) in cases {
        let mut data = Vec::new();
        append_patch_record(&mut data, 0, point_count, 4);
        let object = patch_stream(shading_type, data, [8, 8, 8], patch_decode(), None);

        let shading = Shading::from_dictionary(&object, &PassthroughResolver)
            .expect("patch mesh should parse");
        assert!(matches!(
            shading,
            Shading::PatchMesh { ref patches, .. } if patches.len() == 1
        ));
    }
}

#[test]
fn patch_parser_uses_only_the_low_two_flag_bits() {
    let mut data = Vec::new();
    append_patch_record(&mut data, 0, 12, 4);
    append_patch_record(&mut data, 0b1111_1101, 8, 2);
    let object = patch_stream(6, data, [8, 8, 8], patch_decode(), None);

    let shading =
        Shading::from_dictionary(&object, &PassthroughResolver).expect("patches should parse");
    assert!(matches!(
        shading,
        Shading::PatchMesh { ref patches, .. } if patches.len() == 2
    ));
}

#[test]
fn rejects_invalid_patch_widths_and_decode_arity() {
    let error_cases = [
        patch_stream(6, Vec::new(), [3, 8, 8], patch_decode(), None),
        patch_stream(6, Vec::new(), [8, 3, 8], patch_decode(), None),
        patch_stream(6, Vec::new(), [8, 8, 1], patch_decode(), None),
        patch_stream(
            6,
            Vec::new(),
            [8, 8, 8],
            vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
            None,
        ),
        patch_stream(
            6,
            Vec::new(),
            [8, 8, 8],
            patch_decode(),
            Some(type_2_rgb_function()),
        ),
    ];

    for object in error_cases {
        assert!(matches!(
            Shading::from_dictionary(&object, &PassthroughResolver),
            Err(PdfShadingError::InvalidShadingMeshData { .. })
        ));
    }
}
