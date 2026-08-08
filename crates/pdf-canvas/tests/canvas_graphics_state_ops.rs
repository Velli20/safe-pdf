#![allow(clippy::expect_used)]

mod common;

use std::{collections::HashMap, rc::Rc};

use common::{content_stream, replay};
use pdf_canvas::{error::PdfCanvasError, pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_content_stream::ContentStream;
use pdf_document::page::PdfPage;
use pdf_graphics::{DashPattern, MaskMode, rect::Rect};
use pdf_resources::{
    external_graphics_state::{ExternalGraphicsState, ExternalGraphicsStateKey},
    form::FormXObject,
    resource::Resource,
    resources::Resources,
    soft_mask::SoftMask,
};

fn render(
    stream: &ContentStream,
    resources: &Resources,
) -> Result<RecordingCanvas, PdfCanvasError> {
    let page = PdfPage::default();
    let mut recording = RecordingCanvas::new(100.0, 100.0);
    {
        let mut canvas = PdfCanvas::new(&mut recording, &page, None)?;
        canvas.render_content_stream(stream, None, None, Some(resources), None)?;
    }
    Ok(recording)
}

fn graphics_state_resource(params: Vec<ExternalGraphicsStateKey>) -> Resources {
    Resources {
        ext_g_states: HashMap::from([(
            "GS0".to_string(),
            Resource::ExternalGraphicsState(Rc::new(ExternalGraphicsState { params })),
        )]),
        ..Default::default()
    }
}

#[test]
fn unmatched_restore_graphics_state_is_ignored() {
    let resources = Resources::default();
    let stream = content_stream(1, b"Q q Q Q");

    let recording = render(&stream, &resources).expect("unmatched restores should be ignored");

    let observer = replay(&recording);
    assert_eq!(observer.save_count, 2);
    assert_eq!(observer.restore_count, 2);
}

#[test]
fn external_graphics_state_dash_pattern_is_used_for_strokes() {
    let resources = graphics_state_resource(vec![ExternalGraphicsStateKey::DashPattern(
        DashPattern::new(&[3.0, 1.0], 2.0)
            .expect("dash pattern should be valid")
            .expect("dash pattern should be present"),
    )]);
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
        bbox: Rect {
            left: 5.0,
            top: 10.0,
            right: 5.0,
            bottom: 20.0,
        },
        matrix: None,
        resources: None,
        content_stream: ContentStream {
            operators: Vec::new(),
            id: 2,
        },
    };
    let resources = graphics_state_resource(vec![ExternalGraphicsStateKey::SoftMask(Some(
        Box::new(SoftMask {
            mask_type: MaskMode::Alpha,
            shape: Rc::new(form),
        }),
    ))]);
    let stream = content_stream(1, b"/GS0 gs");

    let recording = render(&stream, &resources).expect("zero-area soft mask should be ignored");

    let observer = replay(&recording);
    assert_eq!(observer.begin_mask_count, 0);
}
