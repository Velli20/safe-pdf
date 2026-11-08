#![allow(clippy::expect_used)]

mod common;

use std::{collections::HashMap, sync::Arc};

use common::{content_stream, replay};
use pdf_canvas::{pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_content_stream::ContentStream;
use pdf_content_stream_operators::variants::PdfOperatorVariant;
use pdf_document::page::PdfPage;
use pdf_graphics::{BlendMode, MaskMode, PixelFormat, rect::Rect};
use pdf_image::PdfImage;
use pdf_object_reader::{
    EmptySource, ObjectHandle, dictionary::Dictionary, object_variant::ObjectVariant,
    pdf_name::PdfName,
};
use pdf_resources::{
    external_graphics_state::{ExternalGraphicsState, ExternalGraphicsStateKey},
    form::{FormGroup, FormXObject},
    pattern::{PaintType, Pattern, TilingType},
    resource::Resource,
    resources::Resources,
    soft_mask::{MaskTransfer, SoftMask},
};
use pdf_text_engine::bundled_font_system;

fn render(
    recording: &mut RecordingCanvas,
    stream: &ContentStream,
    resources: Option<Arc<Resources>>,
) -> Result<(), pdf_canvas::error::PdfCanvasError> {
    let page = PdfPage {
        media_box: Some(Rect::new(100.0, 100.0)),
        ..PdfPage::default()
    };
    let mut canvas = PdfCanvas::new(recording, &page, None, bundled_font_system())?;
    canvas.render_content_stream(stream, None, None, resources, None)
}

fn form_resource(name: &str, stream: ContentStream) -> Resources {
    Resources {
        xobjects: HashMap::from([(
            PdfName::from_bytes(name),
            ObjectHandle::from(Resource::from(FormXObject {
                bbox: Some(Rect {
                    left: 0.0,
                    top: 0.0,
                    right: 10.0,
                    bottom: 10.0,
                }),
                matrix: None,
                resources: None,
                content_stream: stream,
                group: None,
            })),
        )]),
        ..Default::default()
    }
}

fn image_dictionary() -> Dictionary {
    Dictionary::from_iter(std::collections::BTreeMap::from([
        (
            Vec::from(b"Subtype"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"Image",
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        ),
        (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(1)),
        (
            Vec::from(b"ColorSpace"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"DeviceGray",
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        ),
        (
            Vec::from(b"Decode"),
            ObjectVariant::Array(
                vec![ObjectVariant::Integer(1.into()), ObjectVariant::Integer(0)].into(),
            ),
        ),
        (Vec::from(b"Height"), ObjectVariant::Integer(1)),
        (Vec::from(b"Width"), ObjectVariant::Integer(4)),
    ]))
}

fn inline_image() -> PdfImage {
    PdfImage::inline(
        Dictionary::from_iter(std::collections::BTreeMap::from([
            (Vec::from(b"BPC"), ObjectVariant::Integer(1)),
            (
                Vec::from(b"CS"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"D"),
                ObjectVariant::Array(
                    vec![ObjectVariant::Integer(1.into()), ObjectVariant::Integer(0)].into(),
                ),
            ),
            (Vec::from(b"H"), ObjectVariant::Integer(1)),
            (Vec::from(b"W"), ObjectVariant::Integer(4)),
        ])),
        vec![0b1010_0000],
        &mut pdf_object_reader::ObjectReader::new(EmptySource).session(),
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
fn does_not_reenter_an_active_stream() {
    let root = content_stream(1, b"/Self Do");
    let resources = Arc::new(form_resource(
        "Self",
        ContentStream {
            operators: root.operators.clone(),
            id: root.id,
        },
    ));
    let mut recording = RecordingCanvas::new(100.0, 100.0);

    render(&mut recording, &root, Some(Arc::clone(&resources)))
        .expect("a self-referencing stream should render once");
    render(&mut recording, &root, Some(Arc::clone(&resources)))
        .expect("active IDs should be released after rendering");

    let observer = replay(&recording);
    assert_eq!(observer.save_count, 2);
    assert_eq!(observer.restore_count, 2);
}

#[test]
fn does_not_reenter_an_active_stream_from_branches() {
    let root = content_stream(2, b"/Self Do /Self Do");
    let resources = Arc::new(form_resource(
        "Self",
        ContentStream {
            operators: root.operators.clone(),
            id: root.id,
        },
    ));
    let mut recording = RecordingCanvas::new(100.0, 100.0);

    render(&mut recording, &root, Some(Arc::clone(&resources)))
        .expect("a branching self-reference should render once");

    let observer = replay(&recording);
    assert_eq!(observer.save_count, 1);
    assert_eq!(observer.restore_count, 1);
}

#[test]
fn still_renders_nested_streams_with_distinct_ids() {
    let root = content_stream(1, b"/Child Do");
    let child = content_stream(2, b"q Q");
    let resources = Arc::new(form_resource("Child", child));
    let mut recording = RecordingCanvas::new(100.0, 100.0);

    render(&mut recording, &root, Some(Arc::clone(&resources)))
        .expect("distinct nested stream should render");

    let observer = replay(&recording);
    assert_eq!(observer.save_count, 3);
    assert_eq!(observer.restore_count, 3);
}

#[test]
fn releases_render_state_after_an_operator_error() {
    let failing = content_stream(7, b"/Missing Do");
    let recursive = content_stream(7, b"/Self Do");
    let resources = Arc::new(form_resource(
        "Self",
        ContentStream {
            operators: recursive.operators.clone(),
            id: recursive.id,
        },
    ));
    let mut recording = RecordingCanvas::new(100.0, 100.0);
    let page = PdfPage {
        media_box: Some(Rect::new(100.0, 100.0)),
        ..PdfPage::default()
    };
    let mut canvas = PdfCanvas::new(&mut recording, &page, None, bundled_font_system())
        .expect("canvas should be constructed");

    let result = canvas.render_content_stream(&failing, None, None, None, None);
    assert!(matches!(
        result,
        Err(pdf_canvas::error::PdfCanvasError::PageResourcesMissing)
    ));

    canvas
        .render_content_stream(&recursive, None, None, Some(Arc::clone(&resources)), None)
        .expect("the stream should be admitted again after an error");
    drop(canvas);

    let observer = replay(&recording);
    assert_eq!(observer.save_count, 2);
    assert_eq!(observer.restore_count, 2);
}

#[test]
fn unavailable_image_xobject_is_a_no_op() {
    let stream = content_stream(1, b"/Im Do");
    let resources = Arc::new(Resources {
        xobjects: HashMap::from([(
            PdfName::from(b"Im"),
            ObjectHandle::from(Resource::UnavailableImage),
        )]),
        ..Default::default()
    });
    let mut recording = RecordingCanvas::new(100.0, 100.0);

    render(&mut recording, &stream, Some(Arc::clone(&resources)))
        .expect("an unavailable image should not abort page rendering");

    let observer = replay(&recording);
    assert!(observer.images.is_empty());
    assert!(observer.inline_images.is_empty());
}

#[test]
fn inline_image_render_path_matches_image_xobject_path() {
    let image = pdf_object_reader::ObjectReader::new(pdf_object_reader::EmptySource)
        .read::<Resource>(&pdf_object_reader::object_variant::ObjectVariant::Stream(
            pdf_object_reader::stream::StreamObject::new(
                1,
                0,
                image_dictionary(),
                vec![0b1010_0000],
            ),
        ))
        .expect("image XObject should be read");
    let graphics_state = Resource::ExternalGraphicsState(pdf_object_reader::ObjectHandle::from(
        ExternalGraphicsState {
            params: vec![ExternalGraphicsStateKey::BlendMode(BlendMode::Multiply)],
        },
    ));
    let resources = Arc::new(Resources {
        ext_g_states: HashMap::from([(PdfName::from(b"GS"), graphics_state)]),
        xobjects: HashMap::from([(PdfName::from(b"Im"), ObjectHandle::from(image))]),
        ..Default::default()
    });

    let xobject_stream = content_stream(1, b"/GS gs 2 0 0 3 10 20 cm /Im Do");
    let mut inline_stream = content_stream(2, b"/GS gs 2 0 0 3 10 20 cm");
    inline_stream
        .operators
        .push(PdfOperatorVariant::InlineImage(Arc::new(inline_image())));

    let mut xobject_recording = RecordingCanvas::new(100.0, 100.0);
    render(
        &mut xobject_recording,
        &xobject_stream,
        Some(Arc::clone(&resources)),
    )
    .expect("image XObject should render");

    let mut inline_recording = RecordingCanvas::new(100.0, 100.0);
    render(
        &mut inline_recording,
        &inline_stream,
        Some(Arc::clone(&resources)),
    )
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

#[test]
fn image_soft_mask_replaces_the_graphics_state_soft_mask() {
    let mut dictionary = image_dictionary();
    dictionary.insert(
        Vec::from(b"SMask"),
        ObjectVariant::Stream(pdf_object_reader::stream::StreamObject::new(
            2,
            0,
            image_dictionary(),
            vec![0b1100_0000],
        )),
    );
    let image = pdf_object_reader::ObjectReader::new(EmptySource)
        .read::<Resource>(&ObjectVariant::Stream(
            pdf_object_reader::stream::StreamObject::new(1, 0, dictionary, vec![0b1010_0000]),
        ))
        .expect("image XObject should be read");
    // A luminosity mask whose group paints nothing is black, so it would hide everything.
    let empty_group = FormXObject {
        bbox: Some(Rect::new(10.0, 10.0)),
        matrix: None,
        resources: None,
        content_stream: content_stream(3, b"q Q"),
        group: Some(FormGroup::Transparency { isolated: false }),
    };
    let graphics_state =
        Resource::ExternalGraphicsState(ObjectHandle::from(ExternalGraphicsState {
            params: vec![ExternalGraphicsStateKey::SoftMask(Some(
                ObjectHandle::from(SoftMask {
                    mask_type: MaskMode::Luminosity,
                    transfer: MaskTransfer::Identity,
                    shape: empty_group.into(),
                }),
            ))],
        }));
    let resources = Arc::new(Resources {
        ext_g_states: HashMap::from([(PdfName::from(b"GS0"), graphics_state)]),
        xobjects: HashMap::from([(PdfName::from(b"Im"), ObjectHandle::from(image))]),
        ..Default::default()
    });

    let mut recording = RecordingCanvas::new(100.0, 100.0);
    render(
        &mut recording,
        &content_stream(4, b"/GS0 gs 10 0 0 10 0 0 cm /Im Do"),
        Some(resources),
    )
    .expect("soft-masked image should render");

    let observer = replay(&recording);
    assert_eq!(observer.begin_mask_count, 0);
    assert_eq!(observer.images.len(), 1);
}

#[test]
fn image_mask_paints_the_fill_pattern_through_its_stencil() {
    let mut dictionary = image_dictionary();
    dictionary.insert(Vec::from(b"ImageMask"), ObjectVariant::Boolean(true));
    let mask = pdf_object_reader::ObjectReader::new(EmptySource)
        .read::<Resource>(&ObjectVariant::Stream(
            pdf_object_reader::stream::StreamObject::new(1, 0, dictionary, vec![0b1010_0000]),
        ))
        .expect("image mask XObject should be read");
    let pattern = Resource::Pattern(ObjectHandle::from(Pattern::Tiling {
        paint_type: PaintType::Colored,
        tiling_type: TilingType::ConstantSpacing,
        bbox: Rect::new(4.0, 4.0),
        x_step: 4.0,
        y_step: 4.0,
        matrix: None,
        resources: ObjectHandle::from(Resources::default()),
        content_stream: content_stream(3, b"1 0 0 rg 0 0 4 4 re f"),
    }));
    let resources = Arc::new(Resources {
        patterns: HashMap::from([(PdfName::from(b"P0"), pattern)]),
        xobjects: HashMap::from([(PdfName::from(b"Im"), ObjectHandle::from(mask))]),
        ..Default::default()
    });

    let mut recording = RecordingCanvas::new(100.0, 100.0);
    render(
        &mut recording,
        &content_stream(4, b"/Pattern cs /P0 scn 10 0 0 10 0 0 cm /Im Do"),
        Some(resources),
    )
    .expect("pattern-filled image mask should render");

    let observer = replay(&recording);
    assert_eq!(observer.begin_mask_count, 1);
    assert_eq!(observer.fill_colors.len(), 1);
    assert!(observer.images.is_empty());
}
