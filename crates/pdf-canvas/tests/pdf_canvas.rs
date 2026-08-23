#![allow(clippy::expect_used)]

mod common;

use std::{collections::HashMap, rc::Rc, sync::Arc};

use common::{content_stream, replay};
use pdf_canvas::{pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_content_stream::ContentStream;
use pdf_content_stream_operators::variants::PdfOperatorVariant;
use pdf_document::page::PdfPage;
use pdf_graphics::{BlendMode, PixelFormat, rect::Rect};
use pdf_image::InlineImage;
use pdf_object::{
    dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
};
use pdf_resources::{
    external_graphics_state::{ExternalGraphicsState, ExternalGraphicsStateKey},
    form::FormXObject,
    resource::Resource,
    resources::Resources,
};

fn render(
    recording: &mut RecordingCanvas,
    stream: &ContentStream,
    resources: Option<&Resources>,
) -> Result<(), pdf_canvas::error::PdfCanvasError> {
    let page = PdfPage::default();
    let mut canvas = PdfCanvas::new(recording, &page, None)?;
    canvas.render_content_stream(stream, None, None, resources, None)
}

fn form_resource(name: &str, stream: ContentStream) -> Resources {
    Resources {
        xobjects: HashMap::from([(
            name.as_bytes().to_vec(),
            Resource::from(FormXObject {
                bbox: Rect {
                    left: 0.0,
                    top: 0.0,
                    right: 10.0,
                    bottom: 10.0,
                },
                matrix: None,
                resources: None,
                content_stream: stream,
            }),
        )]),
        ..Default::default()
    }
}

fn image_dictionary() -> Dictionary {
    Dictionary::new(std::collections::BTreeMap::from([
        (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(1)),
        (
            Vec::from(b"ColorSpace"),
            ObjectVariant::Name(b"DeviceGray".to_vec()),
        ),
        (
            Vec::from(b"Decode"),
            ObjectVariant::Array(vec![ObjectVariant::Integer(1), ObjectVariant::Integer(0)]),
        ),
        (Vec::from(b"Height"), ObjectVariant::Integer(1)),
        (Vec::from(b"Width"), ObjectVariant::Integer(4)),
    ]))
}

fn inline_image() -> InlineImage {
    InlineImage::new(
        Dictionary::new(std::collections::BTreeMap::from([
            (Vec::from(b"BPC"), ObjectVariant::Integer(1)),
            (
                Vec::from(b"CS"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (
                Vec::from(b"D"),
                ObjectVariant::Array(vec![ObjectVariant::Integer(1), ObjectVariant::Integer(0)]),
            ),
            (Vec::from(b"H"), ObjectVariant::Integer(1)),
            (Vec::from(b"W"), ObjectVariant::Integer(4)),
        ])),
        vec![0b1010_0000],
        &PassthroughResolver,
    )
    .expect("inline image should be constructed")
}

#[test]
fn draw_path_forwards_dash_pattern_to_backend() {
    let stream = content_stream(1, b"[4 2] 1 d 0 0 m 10 0 l S");
    let mut recording = RecordingCanvas::new(100.0, 100.0);

    render(&mut recording, &stream, None).expect("content stream should render");

    let observer = replay(&recording);
    let stroke_style = observer
        .stroke_styles
        .first()
        .expect("one stroke should be recorded");
    let dash_pattern = stroke_style
        .dash_pattern
        .as_ref()
        .expect("stroke should retain its dash pattern");
    assert_eq!(dash_pattern.intervals, vec![4.0, 2.0]);
    assert_eq!(dash_pattern.phase, 1.0);
}

#[test]
fn renders_recursive_stream_until_the_depth_limit() {
    let root = content_stream(1, b"/Self Do");
    let resources = form_resource(
        "Self",
        ContentStream {
            operators: root.operators.clone(),
            id: root.id,
        },
    );
    let mut recording = RecordingCanvas::new(100.0, 100.0);

    render(&mut recording, &root, Some(&resources))
        .expect("recursive render should stop at the depth limit");
    render(&mut recording, &root, Some(&resources))
        .expect("stream depth and active IDs should be released after rendering");

    let observer = replay(&recording);
    assert_eq!(observer.save_count, 80);
    assert_eq!(observer.restore_count, 80);
}

#[test]
fn bounds_branching_recursive_streams_by_invocation_budget() {
    let root = content_stream(2, b"/Self Do /Self Do");
    let resources = form_resource(
        "Self",
        ContentStream {
            operators: root.operators.clone(),
            id: root.id,
        },
    );
    let mut recording = RecordingCanvas::new(100.0, 100.0);

    render(&mut recording, &root, Some(&resources))
        .expect("branching recursion should stop at the invocation budget");

    let observer = replay(&recording);
    assert_eq!(observer.save_count, 4097);
    assert_eq!(observer.restore_count, 4097);
}

#[test]
fn still_renders_nested_streams_with_distinct_ids() {
    let root = content_stream(1, b"/Child Do");
    let child = content_stream(2, b"q Q");
    let resources = form_resource("Child", child);
    let mut recording = RecordingCanvas::new(100.0, 100.0);

    render(&mut recording, &root, Some(&resources)).expect("distinct nested stream should render");

    let observer = replay(&recording);
    assert_eq!(observer.save_count, 3);
    assert_eq!(observer.restore_count, 3);
}

#[test]
fn releases_render_state_after_an_operator_error() {
    let failing = content_stream(7, b"/Missing Do");
    let recursive = content_stream(7, b"/Self Do");
    let resources = form_resource(
        "Self",
        ContentStream {
            operators: recursive.operators.clone(),
            id: recursive.id,
        },
    );
    let mut recording = RecordingCanvas::new(100.0, 100.0);
    let page = PdfPage::default();
    let mut canvas =
        PdfCanvas::new(&mut recording, &page, None).expect("canvas should be constructed");

    let result = canvas.render_content_stream(&failing, None, None, None, None);
    assert!(matches!(
        result,
        Err(pdf_canvas::error::PdfCanvasError::PageResourcesMissing)
    ));

    canvas
        .render_content_stream(&recursive, None, None, Some(&resources), None)
        .expect("rendering should use the full depth budget after an error");
    drop(canvas);

    let observer = replay(&recording);
    assert_eq!(observer.save_count, 41);
    assert_eq!(observer.restore_count, 41);
}

#[test]
fn unavailable_image_xobject_is_a_no_op() {
    let stream = content_stream(1, b"/Im Do");
    let resources = Resources {
        xobjects: HashMap::from([(b"Im".to_vec(), Resource::UnavailableImage)]),
        ..Default::default()
    };
    let mut recording = RecordingCanvas::new(100.0, 100.0);

    render(&mut recording, &stream, Some(&resources))
        .expect("an unavailable image should not abort page rendering");

    let observer = replay(&recording);
    assert!(observer.images.is_empty());
    assert!(observer.inline_images.is_empty());
}

#[test]
fn inline_image_render_path_matches_image_xobject_path() {
    let image = pdf_image::decode_normalized_image(
        &image_dictionary(),
        Arc::new(vec![0b1010_0000]),
        &pdf_object::object_resolver::PassthroughResolver,
        None,
    )
    .expect("image XObject should decode");
    let graphics_state = Resource::ExternalGraphicsState(Rc::new(ExternalGraphicsState {
        params: vec![ExternalGraphicsStateKey::BlendMode(vec![
            BlendMode::Multiply,
        ])],
    }));
    let resources = Resources {
        ext_g_states: HashMap::from([(b"GS".to_vec(), graphics_state)]),
        xobjects: HashMap::from([(b"Im".to_vec(), Resource::from(image))]),
        ..Default::default()
    };

    let xobject_stream = content_stream(1, b"/GS gs 2 0 0 3 10 20 cm /Im Do");
    let mut inline_stream = content_stream(2, b"/GS gs 2 0 0 3 10 20 cm");
    inline_stream
        .operators
        .push(PdfOperatorVariant::InlineImage(Rc::new(inline_image())));

    let mut xobject_recording = RecordingCanvas::new(100.0, 100.0);
    render(&mut xobject_recording, &xobject_stream, Some(&resources))
        .expect("image XObject should render");

    let mut inline_recording = RecordingCanvas::new(100.0, 100.0);
    render(&mut inline_recording, &inline_stream, Some(&resources))
        .expect("inline image should render");

    let xobject_observer = replay(&xobject_recording);
    let inline_observer = replay(&inline_recording);
    let xobject_draw = xobject_observer
        .images
        .first()
        .expect("image XObject draw should be recorded");
    let inline_draw = inline_observer
        .inline_images
        .first()
        .expect("inline image draw should be recorded");

    assert_eq!(xobject_draw, inline_draw);
    assert_eq!(xobject_draw.width, 4);
    assert_eq!(xobject_draw.height, 1);
    assert_eq!(xobject_draw.pixel_format, PixelFormat::Gray8);
    assert_eq!(xobject_draw.blend_mode, Some(BlendMode::Multiply));
    assert_eq!(xobject_draw.data, vec![0x00, 0xFF, 0x00, 0xFF]);
}
