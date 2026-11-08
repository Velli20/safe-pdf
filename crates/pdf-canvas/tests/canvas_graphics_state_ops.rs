#![allow(clippy::expect_used)]

mod common;

use std::{collections::HashMap, sync::Arc};

use common::{content_stream, replay};
use pdf_canvas::{error::PdfCanvasError, pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_content_stream::ContentStream;
use pdf_document::page::PdfPage;
use pdf_graphics::{AlphaConstant, BlendMode, DashPattern, MaskMode, rect::Rect};
use pdf_object_reader::pdf_name::PdfName;
use pdf_resources::{
    external_graphics_state::{ExternalGraphicsState, ExternalGraphicsStateKey},
    form::{FormGroup, FormXObject},
    resource::Resource,
    resources::Resources,
    soft_mask::{MaskTransfer, SoftMask},
};
use pdf_text_engine::bundled_font_system;

fn render(
    stream: &ContentStream,
    resources: &Arc<Resources>,
) -> Result<RecordingCanvas, PdfCanvasError> {
    let page = PdfPage {
        media_box: Some(Rect::new(100.0, 100.0)),
        ..PdfPage::default()
    };
    let mut recording = RecordingCanvas::new(100.0, 100.0);
    {
        let mut canvas = PdfCanvas::new(&mut recording, &page, None, bundled_font_system())?;
        canvas.render_content_stream(stream, None, None, Some(Arc::clone(resources)), None)?;
    }
    Ok(recording)
}

fn graphics_state_resource(params: Vec<ExternalGraphicsStateKey>) -> Resources {
    Resources {
        ext_g_states: HashMap::from([(
            PdfName::from(b"GS0"),
            Resource::ExternalGraphicsState(pdf_object_reader::ObjectHandle::from(
                ExternalGraphicsState { params },
            )),
        )]),
        ..Default::default()
    }
}

#[test]
fn unmatched_restore_graphics_state_is_ignored() {
    let resources = Arc::new(Resources::default());
    let stream = content_stream(1, b"Q q Q Q");

    let recording = render(&stream, &resources).expect("unmatched restores should be ignored");

    let observer = replay(&recording);
    assert_eq!(observer.save_count, 2);
    assert_eq!(observer.restore_count, 2);
}

#[test]
fn external_graphics_state_dash_pattern_is_used_for_strokes() {
    let resources = Arc::new(graphics_state_resource(vec![
        ExternalGraphicsStateKey::DashPattern(
            DashPattern::new(vec![3.0, 1.0], 2.0)
                .expect("dash pattern should be valid")
                .expect("dash pattern should be present"),
        ),
    ]));
    let stream = content_stream(1, b"/GS0 gs 0 0 m 10 0 l S");

    let recording = render(&stream, &resources).expect("content stream should render");

    let observer = replay(&recording);
    let dash_pattern = observer
        .stroke_styles
        .first()
        .expect("one stroke should be recorded")
        .dash_pattern
        .as_ref()
        .expect("stroke should retain its dash pattern");
    assert_eq!(dash_pattern.intervals, vec![3.0, 1.0]);
    assert_eq!(dash_pattern.phase, 2.0);
}

#[test]
fn soft_mask_form_with_zero_area_bbox_is_ignored() {
    let form = FormXObject {
        bbox: Some(Rect {
            left: 5.0,
            top: 10.0,
            right: 5.0,
            bottom: 20.0,
        }),
        matrix: None,
        resources: None,
        content_stream: ContentStream {
            operators: Vec::new(),
            id: 2,
        },
        group: None,
    };
    let resources = Arc::new(graphics_state_resource(vec![
        ExternalGraphicsStateKey::SoftMask(Some(pdf_object_reader::ObjectHandle::from(SoftMask {
            mask_type: MaskMode::Alpha,
            transfer: MaskTransfer::Identity,
            shape: form.into(),
        }))),
    ]));
    let stream = content_stream(1, b"/GS0 gs");

    let recording = render(&stream, &resources).expect("zero-area soft mask should be ignored");

    let observer = replay(&recording);
    assert_eq!(observer.begin_mask_count, 0);
}

#[test]
fn soft_mask_applies_to_a_transparency_group_as_one_object() {
    let mask_group = FormXObject {
        bbox: Some(Rect {
            left: 0.0,
            top: 0.0,
            right: 10.0,
            bottom: 10.0,
        }),
        matrix: None,
        resources: None,
        content_stream: content_stream(2, b"0 0 10 10 re f"),
        group: Some(FormGroup::Transparency { isolated: false }),
    };
    let group = FormXObject {
        bbox: None,
        matrix: None,
        resources: Some(pdf_object_reader::ObjectHandle::from(
            graphics_state_resource(vec![ExternalGraphicsStateKey::SoftMask(None)]),
        )),
        content_stream: content_stream(3, b"/GS0 gs 0 0 5 5 re f 5 5 5 5 re f"),
        group: Some(FormGroup::Transparency { isolated: false }),
    };
    let mut resources = graphics_state_resource(vec![ExternalGraphicsStateKey::SoftMask(Some(
        pdf_object_reader::ObjectHandle::from(SoftMask {
            mask_type: MaskMode::Alpha,
            transfer: MaskTransfer::Identity,
            shape: mask_group.into(),
        }),
    ))]);
    resources.xobjects = HashMap::from([(
        PdfName::from(b"Fm0"),
        pdf_object_reader::ObjectHandle::from(Resource::from(group)),
    )]);
    let stream = content_stream(1, b"/GS0 gs /Fm0 Do");

    let recording = render(&stream, &Arc::new(resources)).expect("masked group should render");

    let observer = replay(&recording);
    assert_eq!(observer.begin_mask_count, 1);
    assert_eq!(observer.fill_colors.len(), 2);
}

#[test]
fn transparency_group_composites_with_the_alpha_and_blend_mode_active_at_do() {
    // The group's own `gs` restores full opacity and Normal blending for its contents.
    let group = FormXObject {
        bbox: None,
        matrix: None,
        resources: Some(pdf_object_reader::ObjectHandle::from(
            graphics_state_resource(vec![
                ExternalGraphicsStateKey::NonStrokingAlpha(AlphaConstant::new(1.0)),
                ExternalGraphicsStateKey::BlendMode(BlendMode::Normal),
            ]),
        )),
        content_stream: content_stream(2, b"/GS0 gs 0 0 5 5 re f"),
        group: Some(FormGroup::Transparency { isolated: false }),
    };
    let mut resources = graphics_state_resource(vec![
        ExternalGraphicsStateKey::NonStrokingAlpha(AlphaConstant::new(0.5)),
        ExternalGraphicsStateKey::BlendMode(BlendMode::Screen),
    ]);
    resources.xobjects = HashMap::from([(
        PdfName::from(b"Fm0"),
        pdf_object_reader::ObjectHandle::from(Resource::from(group)),
    )]);
    let stream = content_stream(1, b"/GS0 gs /Fm0 Do");

    let recording = render(&stream, &Arc::new(resources)).expect("group should render");

    let observer = replay(&recording);
    assert_eq!(observer.group_layers, vec![(0.5, Some(BlendMode::Screen))]);
    assert_eq!(observer.begin_mask_count, 0);
    let alphas: Vec<f32> = observer.fill_colors.iter().map(|color| color.a).collect();
    assert_eq!(alphas, vec![1.0]);
}

#[test]
fn isolated_transparency_group_paints_its_blend_modes_in_its_own_layer() {
    // SoftLight inside an isolated group blends with a transparent backdrop, not the page.
    let group = FormXObject {
        bbox: None,
        matrix: None,
        resources: Some(pdf_object_reader::ObjectHandle::from(
            graphics_state_resource(vec![ExternalGraphicsStateKey::BlendMode(
                BlendMode::SoftLight,
            )]),
        )),
        content_stream: content_stream(2, b"/GS0 gs 0 0 5 5 re f"),
        group: Some(FormGroup::Transparency { isolated: true }),
    };
    let mut resources = graphics_state_resource(Vec::new());
    resources.xobjects = HashMap::from([(
        PdfName::from(b"Fm0"),
        pdf_object_reader::ObjectHandle::from(Resource::from(group)),
    )]);
    let stream = content_stream(1, b"/Fm0 Do");

    let recording = render(&stream, &Arc::new(resources)).expect("group should render");

    let observer = replay(&recording);
    assert_eq!(observer.group_layers, vec![(1.0, None)]);
    assert_eq!(observer.fill_colors.len(), 1);
}
