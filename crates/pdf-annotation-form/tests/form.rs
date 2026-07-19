#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions
)]

use std::{sync::Arc, time::Instant};

use pdf_annotation_form::{
    AnnotationController, AnnotationInteractionError, AnnotationInteractionResult,
    AnnotationPointerMove, AnnotationPointerPress, AnnotationViewport, WidgetEditor,
};
use pdf_annotation_types::{Annotation, AnnotationKind, WidgetFieldValue};
use pdf_canvas::{
    canvas_backend::{CanvasBackend, Image, Shader},
    recording_canvas::RecordingCanvas,
    stroke_style::StrokeStyle,
};
use pdf_document::reader::PdfReader;
use pdf_graphics::{
    BlendMode, MaskMode, PathFillType, color::Color, pdf_path::PathVerb, pdf_path::PdfPath,
    point::Point, rect::Rect, transform::Transform,
};

trait TestAnnotationController {
    fn test_pointer_pressed(
        &mut self,
        page_index: usize,
        document: &mut pdf_document::document::PdfDocument,
        viewport: AnnotationViewport,
        position: Point,
        timestamp: Instant,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError>;

    fn test_pointer_moved(
        &mut self,
        page_index: usize,
        document: &mut pdf_document::document::PdfDocument,
        viewport: AnnotationViewport,
        position: Point,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError>;
}

impl TestAnnotationController for AnnotationController {
    fn test_pointer_pressed(
        &mut self,
        page_index: usize,
        document: &mut pdf_document::document::PdfDocument,
        viewport: AnnotationViewport,
        position: Point,
        timestamp: Instant,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError> {
        self.pointer_pressed(
            document,
            AnnotationPointerPress {
                page_index,
                viewport,
                position,
                timestamp,
            },
        )
    }

    fn test_pointer_moved(
        &mut self,
        page_index: usize,
        document: &mut pdf_document::document::PdfDocument,
        viewport: AnnotationViewport,
        position: Point,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError> {
        self.pointer_moved(
            document,
            AnnotationPointerMove {
                page_index,
                viewport,
                position,
            },
        )
    }
}

#[derive(Default)]
struct CountingCanvas {
    fill_count: usize,
    stroke_count: usize,
    last_fill_path: Option<PdfPath>,
    fill_paths: Vec<PdfPath>,
    fill_colors: Vec<Color>,
}

impl CanvasBackend for CountingCanvas {
    fn fill_path(
        &mut self,
        path: &PdfPath,
        _fill_type: PathFillType,
        color: Color,
        _shader: &Option<Shader>,
        _blend_mode: Option<BlendMode>,
    ) -> Result<(), pdf_canvas::error::PdfCanvasError> {
        self.fill_count += 1;
        self.last_fill_path = Some(path.clone());
        self.fill_paths.push(path.clone());
        self.fill_colors.push(color);
        Ok(())
    }

    fn stroke_path(
        &mut self,
        _path: &PdfPath,
        _color: Color,
        _line_width: f32,
        _stroke_style: &StrokeStyle,
        _shader: &Option<Shader>,
        _blend_mode: Option<BlendMode>,
    ) -> Result<(), pdf_canvas::error::PdfCanvasError> {
        self.stroke_count += 1;
        Ok(())
    }

    fn set_clip_region(
        &mut self,
        _path: &PdfPath,
        _mode: PathFillType,
    ) -> Result<(), pdf_canvas::error::PdfCanvasError> {
        Ok(())
    }

    fn width(&self) -> f32 {
        200.0
    }

    fn height(&self) -> f32 {
        100.0
    }

    fn save(&mut self) -> Result<(), pdf_canvas::error::PdfCanvasError> {
        Ok(())
    }

    fn restore(&mut self) -> Result<(), pdf_canvas::error::PdfCanvasError> {
        Ok(())
    }

    fn draw_image_rect(
        &mut self,
        _image: &Image<'_>,
        _blend_mode: Option<BlendMode>,
        _dest_rect: Rect,
        _image_rotation: Option<f32>,
    ) -> Result<(), pdf_canvas::error::PdfCanvasError> {
        Ok(())
    }

    fn begin_mask_layer(
        &mut self,
        _mask: &Arc<RecordingCanvas>,
        _transform: &Transform,
        _mask_mode: MaskMode,
    ) -> Result<(), pdf_canvas::error::PdfCanvasError> {
        Ok(())
    }

    fn end_mask_layer(
        &mut self,
        _mask: &Arc<RecordingCanvas>,
        _transform: &Transform,
        _mask_mode: MaskMode,
    ) -> Result<(), pdf_canvas::error::PdfCanvasError> {
        Ok(())
    }
}

fn build_pdf(annotation_body: &[u8], appearance_objects: Vec<(usize, Vec<u8>)>) -> Vec<u8> {
    build_pdf_with_annotations(vec![(4, annotation_body.to_vec())], appearance_objects)
}

fn build_pdf_with_annotations(
    annotation_objects: Vec<(usize, Vec<u8>)>,
    appearance_objects: Vec<(usize, Vec<u8>)>,
) -> Vec<u8> {
    let annotation_refs = annotation_objects
        .iter()
        .map(|(number, _)| format!("{number} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        (1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()),
        (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec()),
        (
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Resources << >> /Annots [{annotation_refs}] >>"
            )
            .into_bytes(),
        ),
    ];
    objects.extend(annotation_objects);
    objects.extend(appearance_objects);

    let max_object_number = objects.iter().map(|(number, _)| *number).max().unwrap_or(0);
    let mut offsets = vec![0; max_object_number + 1];
    let mut data = Vec::new();
    data.extend_from_slice(b"%PDF-1.7\n");

    for (number, body) in objects {
        offsets[number] = data.len();
        data.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        data.extend_from_slice(&body);
        data.extend_from_slice(b"\nendobj\n");
    }

    let xref_offset = data.len();
    data.extend_from_slice(format!("xref\n0 {}\n", max_object_number + 1).as_bytes());
    data.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        data.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    data.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF",
            max_object_number + 1
        )
        .as_bytes(),
    );
    data
}

fn appearance_stream(content: &[u8], resources: &[u8]) -> Vec<u8> {
    let mut stream = Vec::new();
    stream.extend_from_slice(
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 100 30] /Resources {} /Length {} >>\nstream\n",
            String::from_utf8_lossy(resources),
            content.len()
        )
        .as_bytes(),
    );
    stream.extend_from_slice(content);
    stream.extend_from_slice(b"\nendstream");
    stream
}

fn path_bounds(path: &PdfPath) -> Rect {
    let mut left = f32::INFINITY;
    let mut top = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    let mut bottom = f32::NEG_INFINITY;

    for verb in &path.verbs {
        let points: &[(f32, f32)] = match *verb {
            PathVerb::MoveTo { x, y } | PathVerb::LineTo { x, y } => &[(x, y)],
            PathVerb::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => &[(x1, y1), (x2, y2), (x3, y3)],
            PathVerb::QuadTo { x1, y1, x2, y2 } => &[(x1, y1), (x2, y2)],
            PathVerb::Close => &[],
        };
        for (x, y) in points {
            left = left.min(*x);
            top = top.min(*y);
            right = right.max(*x);
            bottom = bottom.max(*y);
        }
    }

    Rect {
        left,
        top,
        right,
        bottom,
    }
}

fn approx_eq(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.001,
        "expected {actual} to be approximately {expected}"
    );
}

fn widget_value(annotation: &Annotation) -> Option<&[u8]> {
    let AnnotationKind::Widget(widget) = &annotation.kind else {
        return None;
    };
    let WidgetFieldValue::Bytes(value) = widget.value.as_ref()? else {
        return None;
    };
    Some(value)
}

fn widget_values(annotation: &Annotation) -> Vec<&[u8]> {
    let AnnotationKind::Widget(widget) = &annotation.kind else {
        return Vec::new();
    };
    match widget.value.as_ref() {
        Some(WidgetFieldValue::Bytes(value)) => vec![value],
        Some(WidgetFieldValue::Array(values)) => values
            .iter()
            .filter_map(|value| {
                let WidgetFieldValue::Bytes(value) = value else {
                    return None;
                };
                Some(value.as_slice())
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn widget_selected_indices(annotation: &Annotation) -> Vec<usize> {
    let AnnotationKind::Widget(widget) = &annotation.kind else {
        return Vec::new();
    };
    widget.selected_option_indices()
}

#[test]
fn checkbox_editor_uses_nonstandard_on_state() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /FT /Btn /Rect [10 20 110 50] /V /Off /AS /Off /AP << /N << /Off 5 0 R /1 6 0 R >> >> >>";
    let off_appearance = appearance_stream(b"1 0 0 rg 0 0 100 30 re f", b"<< >>");
    let on_appearance = appearance_stream(b"0 1 0 rg 0 0 100 30 re f", b"<< >>");
    let pdf = build_pdf(annotation, vec![(5, off_appearance), (6, on_appearance)]);
    let mut document = PdfReader
        .read_from_bytes(&pdf, None)
        .expect("PDF should parse");
    let id = document
        .pages
        .first()
        .expect("page should exist")
        .annotations
        .as_deref()
        .and_then(|annotations| annotations.first())
        .expect("annotation")
        .id();

    WidgetEditor::new(&mut document)
        .set_checkbox_checked(0, id, true)
        .expect("checkbox should check");

    let page = document.pages.first().expect("page should exist");
    let annotation = page.annotation(id).expect("annotation");
    assert_eq!(annotation.appearance_state.as_deref(), Some("1"));
    assert_eq!(widget_value(annotation), Some(b"1".as_slice()));
}

#[test]
fn radio_editor_selects_one_in_inherited_field_group() {
    let first = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 60 50] /AS /A /Parent 6 0 R /AP << /N << /Off 7 0 R /A 8 0 R >> >> >>";
    let second = b"<< /Type /Annot /Subtype /Widget /Rect [70 20 120 50] /AS /Off /Parent 6 0 R /AP << /N << /Off 9 0 R /B 10 0 R >> >> >>";
    let field = b"<< /Parent 11 0 R /Ff 32768 /V /A /Kids [4 0 R 5 0 R] >>";
    let field_parent = b"<< /FT /Btn >>";
    let pdf = build_pdf_with_annotations(
        vec![
            (4, first.to_vec()),
            (5, second.to_vec()),
            (6, field.to_vec()),
            (11, field_parent.to_vec()),
        ],
        vec![
            (7, appearance_stream(b"1 0 0 rg 0 0 100 30 re f", b"<< >>")),
            (8, appearance_stream(b"0 1 0 rg 0 0 100 30 re f", b"<< >>")),
            (9, appearance_stream(b"1 0 0 rg 0 0 100 30 re f", b"<< >>")),
            (10, appearance_stream(b"0 0 1 rg 0 0 100 30 re f", b"<< >>")),
        ],
    );
    let mut document = PdfReader
        .read_from_bytes(&pdf, None)
        .expect("PDF should parse");
    let page = document.pages.first().expect("page should exist");
    let annotations = page.annotations.as_deref().expect("annotations");
    let first_id = annotations.first().expect("first radio").id();
    let second_id = annotations.get(1).expect("second radio").id();

    WidgetEditor::new(&mut document)
        .set_radio_selected(0, second_id, true)
        .expect("radio should select");

    let page = document.pages.first().expect("page should exist");
    let first = page.annotation(first_id).expect("first radio");
    let second = page.annotation(second_id).expect("second radio");
    assert_eq!(first.appearance_state.as_deref(), Some("Off"));
    assert_eq!(second.appearance_state.as_deref(), Some("B"));
    assert_eq!(widget_value(first), Some(b"B".as_slice()));
    assert_eq!(widget_value(second), Some(b"B".as_slice()));
    let AnnotationKind::Widget(first_widget) = &first.kind else {
        panic!("first annotation should be a widget");
    };
    assert_eq!(first_widget.field_id, Some(6));
}

#[test]
fn checkbox_editor_synchronizes_field_widgets_across_pages() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 60 50] /AS /Off /Parent 6 0 R /AP << /N << /Off 7 0 R /Yes 8 0 R >> >> >>";
    let field = b"<< /FT /Btn /V /Off /Kids [4 0 R] >>";
    let appearances = || {
        vec![
            (7, appearance_stream(b"", b"<< >>")),
            (8, appearance_stream(b"", b"<< >>")),
        ]
    };
    let first_pdf = build_pdf_with_annotations(
        vec![(4, annotation.to_vec()), (6, field.to_vec())],
        appearances(),
    );
    let second_pdf = build_pdf_with_annotations(
        vec![(4, annotation.to_vec()), (6, field.to_vec())],
        appearances(),
    );
    let mut document = PdfReader
        .read_from_bytes(&first_pdf, None)
        .expect("first PDF should parse");
    let second_document = PdfReader
        .read_from_bytes(&second_pdf, None)
        .expect("second PDF should parse");
    document.pages.extend(second_document.pages);
    let second_id = document.pages[1].annotations.as_ref().unwrap()[0].id();

    WidgetEditor::new(&mut document)
        .set_checkbox_checked(1, second_id, true)
        .expect("checkbox should check");

    for page in &document.pages {
        let annotation = &page.annotations.as_ref().unwrap()[0];
        assert_eq!(annotation.appearance_state.as_deref(), Some("Yes"));
        assert_eq!(widget_value(annotation), Some(b"Yes".as_slice()));
    }
}

#[test]
fn radio_editor_synchronizes_field_value_across_pages() {
    let first = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 60 50] /AS /A /Parent 6 0 R /AP << /N << /Off 7 0 R /A 8 0 R >> >> >>";
    let second = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 60 50] /AS /Off /Parent 6 0 R /AP << /N << /Off 7 0 R /B 8 0 R >> >> >>";
    let field = b"<< /FT /Btn /Ff 32768 /V /A /Kids [4 0 R] >>";
    let appearances = || {
        vec![
            (7, appearance_stream(b"", b"<< >>")),
            (8, appearance_stream(b"", b"<< >>")),
        ]
    };
    let first_pdf = build_pdf_with_annotations(
        vec![(4, first.to_vec()), (6, field.to_vec())],
        appearances(),
    );
    let second_pdf = build_pdf_with_annotations(
        vec![(4, second.to_vec()), (6, field.to_vec())],
        appearances(),
    );
    let mut document = PdfReader
        .read_from_bytes(&first_pdf, None)
        .expect("first PDF should parse");
    let second_document = PdfReader
        .read_from_bytes(&second_pdf, None)
        .expect("second PDF should parse");
    document.pages.extend(second_document.pages);
    let second_id = document.pages[1].annotations.as_ref().unwrap()[0].id();

    WidgetEditor::new(&mut document)
        .set_radio_selected(1, second_id, true)
        .expect("radio should select");

    let first = &document.pages[0].annotations.as_ref().unwrap()[0];
    let second = &document.pages[1].annotations.as_ref().unwrap()[0];
    assert_eq!(first.appearance_state.as_deref(), Some("Off"));
    assert_eq!(second.appearance_state.as_deref(), Some("B"));
    assert_eq!(widget_value(first), Some(b"B".as_slice()));
    assert_eq!(widget_value(second), Some(b"B".as_slice()));
}

#[test]
fn radios_in_unison_select_matching_on_states() {
    let first = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 40 50] /AS /Off /Parent 6 0 R /AP << /N << /Off 8 0 R /A 9 0 R >> >> >>";
    let second = b"<< /Type /Annot /Subtype /Widget /Rect [50 20 80 50] /AS /Off /Parent 6 0 R /AP << /N << /Off 10 0 R /A 11 0 R >> >> >>";
    let third = b"<< /Type /Annot /Subtype /Widget /Rect [90 20 120 50] /AS /B /Parent 6 0 R /AP << /N << /Off 12 0 R /B 13 0 R >> >> >>";
    let field = b"<< /FT /Btn /Ff 33587200 /V /B /Kids [4 0 R 5 0 R 7 0 R] >>";
    let pdf = build_pdf_with_annotations(
        vec![
            (4, first.to_vec()),
            (5, second.to_vec()),
            (6, field.to_vec()),
            (7, third.to_vec()),
        ],
        (8..=13)
            .map(|number| (number, appearance_stream(b"", b"<< >>")))
            .collect(),
    );
    let mut document = PdfReader
        .read_from_bytes(&pdf, None)
        .expect("PDF should parse");
    let second_id = document.pages[0].annotations.as_ref().unwrap()[1].id();

    WidgetEditor::new(&mut document)
        .set_radio_selected(0, second_id, true)
        .expect("radio should select");

    let annotations = document.pages[0].annotations.as_ref().unwrap();
    assert_eq!(annotations[0].appearance_state.as_deref(), Some("A"));
    assert_eq!(annotations[1].appearance_state.as_deref(), Some("A"));
    assert_eq!(annotations[2].appearance_state.as_deref(), Some("Off"));
    assert!(
        annotations
            .iter()
            .all(|annotation| widget_value(annotation) == Some(b"A".as_slice()))
    );

    WidgetEditor::new(&mut document)
        .set_radio_selected(0, second_id, false)
        .expect("radio should clear");

    let annotations = document.pages[0].annotations.as_ref().unwrap();
    assert!(
        annotations
            .iter()
            .all(|annotation| annotation.appearance_state.as_deref() == Some("Off"))
    );
    assert!(
        annotations
            .iter()
            .all(|annotation| widget_value(annotation) == Some(b"Off".as_slice()))
    );
}

#[test]
fn radios_without_unison_keep_duplicate_on_states_exclusive() {
    let first = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 40 50] /AS /A /Parent 6 0 R /AP << /N << /Off 7 0 R /A 8 0 R >> >> >>";
    let second = b"<< /Type /Annot /Subtype /Widget /Rect [50 20 80 50] /AS /Off /Parent 6 0 R /AP << /N << /Off 9 0 R /A 10 0 R >> >> >>";
    let field = b"<< /FT /Btn /Ff 32768 /V /A /Kids [4 0 R 5 0 R] >>";
    let pdf = build_pdf_with_annotations(
        vec![
            (4, first.to_vec()),
            (5, second.to_vec()),
            (6, field.to_vec()),
        ],
        (7..=10)
            .map(|number| (number, appearance_stream(b"", b"<< >>")))
            .collect(),
    );
    let mut document = PdfReader
        .read_from_bytes(&pdf, None)
        .expect("PDF should parse");
    let second_id = document.pages[0].annotations.as_ref().unwrap()[1].id();

    WidgetEditor::new(&mut document)
        .set_radio_selected(0, second_id, true)
        .expect("radio should select");

    let annotations = document.pages[0].annotations.as_ref().unwrap();
    assert_eq!(annotations[0].appearance_state.as_deref(), Some("Off"));
    assert_eq!(annotations[1].appearance_state.as_deref(), Some("A"));
}

#[test]
fn radio_activation_honors_no_toggle_to_off() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 60 50] /AS /A /Parent 6 0 R /AP << /N << /Off 7 0 R /A 8 0 R >> >> >>";
    let field = b"<< /FT /Btn /Ff 49152 /V /A /Kids [4 0 R] >>";
    let pdf = build_pdf_with_annotations(
        vec![(4, annotation.to_vec()), (6, field.to_vec())],
        vec![
            (7, appearance_stream(b"", b"<< >>")),
            (8, appearance_stream(b"", b"<< >>")),
        ],
    );
    let mut document = PdfReader
        .read_from_bytes(&pdf, None)
        .expect("PDF should parse");
    let id = document.pages[0].annotations.as_ref().unwrap()[0].id();

    let activation = WidgetEditor::new(&mut document)
        .activate(0, id)
        .expect("radio should activate")
        .expect("radio is a widget");

    assert!(!activation.state_changed);
    assert_eq!(
        document.pages[0].annotations.as_ref().unwrap()[0]
            .appearance_state
            .as_deref(),
        Some("A")
    );
}

#[test]
fn radio_activation_can_toggle_selected_button_off() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 60 50] /AS /A /Parent 6 0 R /AP << /N << /Off 7 0 R /A 8 0 R >> >> >>";
    let field = b"<< /FT /Btn /Ff 32768 /V /A /Kids [4 0 R] >>";
    let pdf = build_pdf_with_annotations(
        vec![(4, annotation.to_vec()), (6, field.to_vec())],
        vec![
            (7, appearance_stream(b"", b"<< >>")),
            (8, appearance_stream(b"", b"<< >>")),
        ],
    );
    let mut document = PdfReader
        .read_from_bytes(&pdf, None)
        .expect("PDF should parse");
    let id = document.pages[0].annotations.as_ref().unwrap()[0].id();

    let activation = WidgetEditor::new(&mut document)
        .activate(0, id)
        .expect("radio should activate")
        .expect("radio is a widget");

    assert!(activation.state_changed);
    let annotation = &document.pages[0].annotations.as_ref().unwrap()[0];
    assert_eq!(annotation.appearance_state.as_deref(), Some("Off"));
    assert_eq!(widget_value(annotation), Some(b"Off".as_slice()));
}

#[test]
fn read_only_button_activation_does_not_change_state() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /FT /Btn /Ff 1 /Rect [10 20 60 50] /AS /Off /AP << /N << /Off 7 0 R /Yes 8 0 R >> >> >>";
    let pdf = build_pdf(
        annotation,
        vec![
            (7, appearance_stream(b"", b"<< >>")),
            (8, appearance_stream(b"", b"<< >>")),
        ],
    );
    let mut document = PdfReader
        .read_from_bytes(&pdf, None)
        .expect("PDF should parse");
    let id = document.pages[0].annotations.as_ref().unwrap()[0].id();

    let activation = WidgetEditor::new(&mut document)
        .activate(0, id)
        .expect("checkbox should activate")
        .expect("checkbox is a widget");

    assert!(!activation.state_changed);
    assert_eq!(
        document.pages[0].annotations.as_ref().unwrap()[0]
            .appearance_state
            .as_deref(),
        Some("Off")
    );

    WidgetEditor::new(&mut document)
        .set_checkbox_checked(0, id, true)
        .expect("explicit editing should update a read-only field");
    assert_eq!(
        document.pages[0].annotations.as_ref().unwrap()[0]
            .appearance_state
            .as_deref(),
        Some("Yes")
    );
}

#[test]
fn annotation_controller_activates_widgets_through_the_document() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /FT /Btn /Rect [10 20 60 50] /AS /Off /AP << /N << /Off 7 0 R /Yes 8 0 R >> >> >>";
    let pdf = build_pdf(
        annotation,
        vec![
            (7, appearance_stream(b"", b"<< >>")),
            (8, appearance_stream(b"", b"<< >>")),
        ],
    );
    let mut document = PdfReader
        .read_from_bytes(&pdf, None)
        .expect("PDF should parse");
    let viewport = AnnotationViewport::from_page(&document.pages[0], 200.0, 100.0)
        .expect("viewport should build");

    let outcome = AnnotationController::default()
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(20.0, 60.0),
            Instant::now(),
        )
        .expect("pointer press should succeed");

    assert!(outcome.consumed);
    assert!(outcome.redraw);
    assert_eq!(
        document.pages[0].annotations.as_ref().unwrap()[0]
            .appearance_state
            .as_deref(),
        Some("Yes")
    );
}

#[test]
fn parses_listbox_choice_state_and_option_pairs() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 2097152 /Rect [10 20 110 80] /DA (/Helvetica 10 Tf 0 g) /Opt [(Alpha) [(b) (Beta)]] /V (Alpha) /I [1] /TI 1 >>";
    let document = parsed_annotation_document(annotation);
    let annotation = document.pages[0]
        .annotations
        .as_ref()
        .unwrap()
        .first()
        .unwrap();
    let AnnotationKind::Widget(widget) = &annotation.kind else {
        panic!("annotation should be a widget");
    };

    assert!(widget.is_multi_select());
    assert!(!widget.is_combo_box());
    assert_eq!(widget.top_index, Some(1));
    assert_eq!(widget.selected_option_indices(), vec![1]);
    let options = widget
        .options
        .as_ref()
        .expect("choice options should parse");
    assert_eq!(options[0].export_value, b"Alpha");
    assert_eq!(options[0].display_value, b"Alpha");
    assert_eq!(options[1].export_value, b"b");
    assert_eq!(options[1].display_value, b"Beta");

    let fallback = b"<< /Type /Annot /Subtype /Widget /FT /Ch /Rect [10 20 110 80] /Opt [(Alpha) [(b) (Beta)]] /V (b) >>";
    let document = parsed_annotation_document(fallback);
    let annotation = &document.pages[0].annotations.as_ref().unwrap()[0];
    assert_eq!(widget_selected_indices(annotation), vec![1]);
}

#[test]
fn listbox_editor_updates_single_selection_and_rejects_multiple_values() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /FT /Ch /Rect [10 20 110 80] /Opt [(Alpha) (Beta) (Gamma)] /V (Alpha) /I [0] >>";
    let mut document = parsed_annotation_document(annotation);
    let id = document.pages[0].annotations.as_ref().unwrap()[0].id();

    WidgetEditor::new(&mut document)
        .set_listbox_selection(0, id, &[2])
        .expect("single listbox selection should update");
    let annotation = &document.pages[0].annotations.as_ref().unwrap()[0];
    assert_eq!(widget_selected_indices(annotation), vec![2]);
    assert_eq!(widget_value(annotation), Some(b"Gamma".as_slice()));

    let error = WidgetEditor::new(&mut document)
        .set_listbox_selection(0, id, &[0, 1])
        .expect_err("single-select listbox should reject multiple values");
    assert!(matches!(
        error,
        pdf_annotation_form::WidgetEditError::MultipleSelectionNotAllowed { .. }
    ));

    WidgetEditor::new(&mut document)
        .set_listbox_selection(0, id, &[])
        .expect("single listbox selection should clear");
    let annotation = &document.pages[0].annotations.as_ref().unwrap()[0];
    assert!(widget_values(annotation).is_empty());
    assert!(widget_selected_indices(annotation).is_empty());
}

#[test]
fn listbox_editor_synchronizes_choice_field_widgets_across_pages() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 110 80] /Parent 6 0 R >>";
    let field = b"<< /FT /Ch /Ff 2097152 /Opt [(Alpha) (Beta) (Gamma)] /V [(Alpha)] /I [0] >>";
    let first_pdf = build_pdf_with_annotations(
        vec![(4, annotation.to_vec()), (6, field.to_vec())],
        Vec::new(),
    );
    let second_pdf = build_pdf_with_annotations(
        vec![(4, annotation.to_vec()), (6, field.to_vec())],
        Vec::new(),
    );
    let mut document = PdfReader
        .read_from_bytes(&first_pdf, None)
        .expect("first PDF should parse");
    let second_document = PdfReader
        .read_from_bytes(&second_pdf, None)
        .expect("second PDF should parse");
    document.pages.extend(second_document.pages);
    let second_id = document.pages[1].annotations.as_ref().unwrap()[0].id();

    WidgetEditor::new(&mut document)
        .set_listbox_selection(1, second_id, &[1, 2])
        .expect("choice field should update across pages");

    for page in &document.pages {
        let annotation = &page.annotations.as_ref().unwrap()[0];
        assert_eq!(widget_selected_indices(annotation), vec![1, 2]);
        assert_eq!(
            widget_values(annotation),
            vec![b"Beta".as_slice(), b"Gamma".as_slice()]
        );
    }
}

#[test]
fn controller_toggles_multi_select_listbox_rows() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 2097152 /Rect [10 20 110 80] /DA (/Helvetica 10 Tf 0 g) /Opt [(Alpha) (Beta) (Gamma)] /V [(Alpha) (Gamma)] /I [0 2] >>";
    let mut document = parsed_annotation_document(annotation);
    let viewport = AnnotationViewport::from_page(&document.pages[0], 200.0, 100.0)
        .expect("viewport should build");
    let mut controller = AnnotationController::default();

    let selected = controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(20.0, 35.0),
            Instant::now(),
        )
        .expect("second row should toggle on");
    assert!(selected.consumed);
    assert!(selected.redraw);
    let annotation = &document.pages[0].annotations.as_ref().unwrap()[0];
    assert_eq!(widget_selected_indices(annotation), vec![0, 1, 2]);
    assert_eq!(
        widget_values(annotation),
        vec![b"Alpha".as_slice(), b"Beta".as_slice(), b"Gamma".as_slice()]
    );

    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(20.0, 25.0),
            Instant::now(),
        )
        .expect("first row should toggle off");
    let annotation = &document.pages[0].annotations.as_ref().unwrap()[0];
    assert_eq!(widget_selected_indices(annotation), vec![1, 2]);
}

#[test]
fn controller_maps_top_index_and_ignores_unused_listbox_space() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /FT /Ch /Rect [10 20 110 80] /DA (/Helvetica 10 Tf) /Opt [(Alpha) (Beta) (Gamma)] /V (Alpha) /I [0] /TI 1 >>";
    let mut document = parsed_annotation_document(annotation);
    let viewport = AnnotationViewport::from_page(&document.pages[0], 200.0, 100.0)
        .expect("viewport should build");
    let mut controller = AnnotationController::default();

    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(20.0, 25.0),
            Instant::now(),
        )
        .expect("first visible row should select option at top index");
    assert_eq!(
        widget_selected_indices(&document.pages[0].annotations.as_ref().unwrap()[0]),
        vec![1]
    );

    let outcome = controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(20.0, 70.0),
            Instant::now(),
        )
        .expect("unused listbox space should be consumed");
    assert!(outcome.consumed);
    assert_eq!(
        widget_selected_indices(&document.pages[0].annotations.as_ref().unwrap()[0]),
        vec![1]
    );
}

#[test]
fn controller_honors_read_only_listboxes_and_ignores_combo_boxes() {
    let read_only = b"<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 1 /Rect [10 20 110 80] /DA (/Helvetica 10 Tf) /Opt [(Alpha) (Beta)] /V (Alpha) /I [0] >>";
    let mut document = parsed_annotation_document(read_only);
    let viewport = AnnotationViewport::from_page(&document.pages[0], 200.0, 100.0)
        .expect("viewport should build");
    let outcome = AnnotationController::default()
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(20.0, 35.0),
            Instant::now(),
        )
        .expect("read-only listbox click should be handled");
    assert!(outcome.consumed);
    assert_eq!(
        widget_selected_indices(&document.pages[0].annotations.as_ref().unwrap()[0]),
        vec![0]
    );

    let combo = b"<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /Rect [10 20 110 80] /Opt [(Alpha) (Beta)] >>";
    let mut document = parsed_annotation_document(combo);
    let viewport = AnnotationViewport::from_page(&document.pages[0], 200.0, 100.0)
        .expect("viewport should build");
    let outcome = AnnotationController::default()
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(20.0, 25.0),
            Instant::now(),
        )
        .expect("combo click should be ignored");
    assert!(!outcome.consumed);
}

#[test]
fn listbox_overlay_uses_top_index_and_clips_rows_to_widget() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 2097152 /Rect [10 20 110 50] /DA (/Helvetica 10 Tf 0 g) /Opt [(A) (B) (C) (D)] /V [(B) (D)] /I [1 3] /TI 1 >>";
    let document = parsed_annotation_document(annotation);
    let page = &document.pages[0];
    let viewport =
        AnnotationViewport::from_page(page, 200.0, 100.0).expect("viewport should build");
    let mut backend = CountingCanvas::default();

    AnnotationController::default()
        .draw_overlay(&mut backend, page, viewport)
        .expect("listbox overlay should draw");

    assert_eq!(backend.fill_count, 2);
    let first = path_bounds(&backend.fill_paths[0]);
    let second = path_bounds(&backend.fill_paths[1]);
    approx_eq(first.top, 50.0);
    approx_eq(first.bottom, 62.0);
    approx_eq(second.top, 74.0);
    approx_eq(second.bottom, 80.0);
}

fn parsed_annotation_document(annotation: &[u8]) -> pdf_document::document::PdfDocument {
    PdfReader
        .read_from_bytes(&build_pdf(annotation, Vec::new()), None)
        .expect("PDF should parse")
}

fn drag_first_annotation(
    document: &mut pdf_document::document::PdfDocument,
    controller: &mut AnnotationController,
    from: Point,
    to: Point,
) {
    let viewport = AnnotationViewport::from_page(&document.pages[0], 200.0, 100.0)
        .expect("viewport should build");
    controller
        .test_pointer_pressed(0, document, viewport, from, Instant::now())
        .expect("pointer press should succeed");
    let outcome = controller
        .test_pointer_moved(0, document, viewport, to)
        .expect("pointer movement should succeed");
    assert!(outcome.consumed);
    assert!(outcome.redraw);
}

fn first_annotation(document: &pdf_document::document::PdfDocument) -> &Annotation {
    &document.pages[0].annotations.as_ref().unwrap()[0]
}

#[test]
fn controller_drags_all_visual_object_annotation_subtypes() {
    let annotations: &[&[u8]] = &[
        b"<< /Type /Annot /Subtype /FreeText /Rect [10 20 30 40] >>",
        b"<< /Type /Annot /Subtype /Text /Rect [10 20 30 40] >>",
        b"<< /Type /Annot /Subtype /Stamp /Rect [10 20 30 40] >>",
        b"<< /Type /Annot /Subtype /Line /Rect [10 20 30 40] /L [10 20 30 40] >>",
        b"<< /Type /Annot /Subtype /Square /Rect [10 20 30 40] >>",
        b"<< /Type /Annot /Subtype /Circle /Rect [10 20 30 40] >>",
        b"<< /Type /Annot /Subtype /Polygon /Rect [10 20 30 40] /Vertices [10 20 30 20 20 40] >>",
        b"<< /Type /Annot /Subtype /PolyLine /Rect [10 20 30 40] /Vertices [10 20 30 20 20 40] >>",
        b"<< /Type /Annot /Subtype /Ink /Rect [10 20 30 40] /InkList [[10 20 30 40]] >>",
    ];

    for annotation in annotations {
        let mut document = parsed_annotation_document(annotation);
        let id = first_annotation(&document).id();
        let mut controller = AnnotationController::default();

        drag_first_annotation(
            &mut document,
            &mut controller,
            Point::new(20.0, 70.0),
            Point::new(30.0, 65.0),
        );

        assert_eq!(controller.selected(), Some(id));
        assert_eq!(
            first_annotation(&document).rect.unwrap().normalized(),
            Rect {
                left: 20.0,
                top: 25.0,
                right: 40.0,
                bottom: 45.0,
            }
        );
    }
}

#[test]
fn controller_translates_free_text_callout_and_line_endpoints() {
    let mut document = parsed_annotation_document(
        b"<< /Type /Annot /Subtype /FreeText /Rect [10 20 30 40] /CL [5 15 10 20 20 30] >>",
    );
    drag_first_annotation(
        &mut document,
        &mut AnnotationController::default(),
        Point::new(20.0, 70.0),
        Point::new(30.0, 65.0),
    );
    let AnnotationKind::FreeText(free_text) = &first_annotation(&document).kind else {
        panic!("expected FreeText annotation");
    };
    assert_eq!(
        free_text.callout_line.as_deref(),
        Some([15.0, 20.0, 20.0, 25.0, 30.0, 35.0].as_slice())
    );

    let mut document = parsed_annotation_document(
        b"<< /Type /Annot /Subtype /Line /Rect [10 20 30 40] /L [10 20 30 40] >>",
    );
    let mut controller = AnnotationController::default();
    drag_first_annotation(
        &mut document,
        &mut controller,
        Point::new(20.0, 70.0),
        Point::new(30.0, 65.0),
    );
    let viewport = AnnotationViewport::from_page(&document.pages[0], 200.0, 100.0).unwrap();
    controller
        .test_pointer_moved(0, &mut document, viewport, Point::new(40.0, 60.0))
        .expect("second movement should succeed");
    let AnnotationKind::Line(line) = &first_annotation(&document).kind else {
        panic!("expected Line annotation");
    };
    assert_eq!(line.line, [30.0, 30.0, 50.0, 50.0]);
}

fn assert_translated_path(path: &PdfPath, closes: bool) {
    let expected = if closes {
        vec![
            PathVerb::MoveTo { x: 20.0, y: 25.0 },
            PathVerb::LineTo { x: 40.0, y: 25.0 },
            PathVerb::LineTo { x: 30.0, y: 45.0 },
            PathVerb::Close,
        ]
    } else {
        vec![
            PathVerb::MoveTo { x: 20.0, y: 25.0 },
            PathVerb::LineTo { x: 40.0, y: 25.0 },
            PathVerb::LineTo { x: 30.0, y: 45.0 },
        ]
    };
    assert_eq!(path.verbs, expected);
    assert_eq!(path.current_point(), Some((30.0, 45.0)));
}

#[test]
fn controller_translates_polygon_polyline_and_all_ink_paths() {
    for (subtype, closes) in [("Polygon", true), ("PolyLine", false)] {
        let annotation = format!(
            "<< /Type /Annot /Subtype /{subtype} /Rect [10 20 30 40] /Vertices [10 20 30 20 20 40] >>"
        );
        let mut document = parsed_annotation_document(annotation.as_bytes());
        drag_first_annotation(
            &mut document,
            &mut AnnotationController::default(),
            Point::new(20.0, 70.0),
            Point::new(30.0, 65.0),
        );
        let path = match &first_annotation(&document).kind {
            AnnotationKind::Polygon(polygon) => &polygon.vertices,
            AnnotationKind::PolyLine(polyline) => &polyline.vertices,
            _ => panic!("expected polygonal annotation"),
        };
        assert_translated_path(path, closes);
    }

    let mut document = parsed_annotation_document(
        b"<< /Type /Annot /Subtype /Ink /Rect [10 20 30 40] /InkList [[10 20 30 40] [15 25 20 30]] >>",
    );
    drag_first_annotation(
        &mut document,
        &mut AnnotationController::default(),
        Point::new(20.0, 70.0),
        Point::new(30.0, 65.0),
    );
    let AnnotationKind::Ink(ink) = &first_annotation(&document).kind else {
        panic!("expected Ink annotation");
    };
    assert_eq!(
        ink.ink_list.strokes[0].verbs,
        vec![
            PathVerb::MoveTo { x: 20.0, y: 25.0 },
            PathVerb::LineTo { x: 40.0, y: 45.0 },
        ]
    );
    assert_eq!(
        ink.ink_list.strokes[1].verbs,
        vec![
            PathVerb::MoveTo { x: 25.0, y: 30.0 },
            PathVerb::LineTo { x: 30.0, y: 35.0 },
        ]
    );
}

#[test]
fn controller_applies_clamped_delta_to_line_geometry() {
    let mut document = parsed_annotation_document(
        b"<< /Type /Annot /Subtype /Line /Rect [170 70 190 90] /L [170 70 190 90] >>",
    );
    drag_first_annotation(
        &mut document,
        &mut AnnotationController::default(),
        Point::new(180.0, 20.0),
        Point::new(280.0, -30.0),
    );
    let annotation = first_annotation(&document);
    assert_eq!(
        annotation.rect.unwrap().normalized(),
        Rect {
            left: 180.0,
            top: 80.0,
            right: 200.0,
            bottom: 100.0,
        }
    );
    let AnnotationKind::Line(line) = &annotation.kind else {
        panic!("expected Line annotation");
    };
    assert_eq!(line.line, [180.0, 80.0, 200.0, 100.0]);
}

#[test]
fn text_markup_is_not_draggable_and_stamp_double_click_does_not_edit() {
    let mut document = parsed_annotation_document(
        b"<< /Type /Annot /Subtype /Highlight /Rect [10 20 30 40] /QuadPoints [10 40 30 40 10 20 30 20] >>",
    );
    let original_rect = first_annotation(&document).rect;
    let viewport = AnnotationViewport::from_page(&document.pages[0], 200.0, 100.0).unwrap();
    let mut controller = AnnotationController::default();
    let outcome = controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(20.0, 70.0),
            Instant::now(),
        )
        .expect("press should succeed");
    assert!(!outcome.consumed);
    assert_eq!(controller.selected(), None);
    assert_eq!(first_annotation(&document).rect, original_rect);

    let mut document =
        parsed_annotation_document(b"<< /Type /Annot /Subtype /Stamp /Rect [10 20 30 40] >>");
    let viewport = AnnotationViewport::from_page(&document.pages[0], 200.0, 100.0).unwrap();
    let mut controller = AnnotationController::default();
    let first_click = Instant::now();
    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(20.0, 70.0),
            first_click,
        )
        .expect("first Stamp click should succeed");
    controller.pointer_released();
    controller
        .test_pointer_pressed(
            0,
            &mut document,
            viewport,
            Point::new(20.0, 70.0),
            first_click + std::time::Duration::from_millis(100),
        )
        .expect("second Stamp click should succeed");
    assert!(!controller.is_editing());
}
