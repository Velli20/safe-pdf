#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions
)]

use std::sync::Arc;

use pdf_annotation_types::Annotation;
use pdf_annotations::{AnnotationInteractionState, AnnotationRenderError, AnnotationRenderer};
use pdf_canvas::{
    canvas_backend::{CanvasBackend, Image, Shader},
    pdf_canvas::PdfCanvas,
    recording_canvas::RecordingCanvas,
    stroke_style::StrokeStyle,
};
use pdf_document::reader::PdfReader;
use pdf_graphics::{
    BlendMode, MaskMode, PathFillType, color::Color, pdf_path::PathVerb, pdf_path::PdfPath,
    rect::Rect, transform::Transform,
};

#[derive(Default)]
struct CountingCanvas {
    fill_count: usize,
    stroke_count: usize,
    last_fill_path: Option<PdfPath>,
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

fn render_first_page(pdf: &[u8]) -> CountingCanvas {
    try_render_first_page(pdf).expect("annotations should render")
}

fn render_first_page_with_state(
    pdf: &[u8],
    interaction_state: AnnotationInteractionState,
) -> CountingCanvas {
    try_render_first_page_with_state(pdf, interaction_state).expect("annotations should render")
}

fn try_render_first_page(pdf: &[u8]) -> Result<CountingCanvas, AnnotationRenderError> {
    try_render_first_page_with_state(pdf, AnnotationInteractionState::Normal)
}

fn try_render_first_page_with_state(
    pdf: &[u8],
    interaction_state: AnnotationInteractionState,
) -> Result<CountingCanvas, AnnotationRenderError> {
    try_render_first_page_with_state_resolver(pdf, |_, _| interaction_state)
}

fn render_first_page_with_state_resolver<F>(pdf: &[u8], resolver: F) -> CountingCanvas
where
    F: FnMut(usize, &Annotation) -> AnnotationInteractionState,
{
    try_render_first_page_with_state_resolver(pdf, resolver).expect("annotations should render")
}

fn try_render_first_page_with_state_resolver<F>(
    pdf: &[u8],
    resolver: F,
) -> Result<CountingCanvas, AnnotationRenderError>
where
    F: FnMut(usize, &Annotation) -> AnnotationInteractionState,
{
    let document = PdfReader
        .read_from_bytes(pdf, None)
        .expect("PDF should parse");
    let page = document.pages.get(0).expect("page should exist");
    let mut backend = CountingCanvas::default();
    let canvas = PdfCanvas::new(&mut backend, page, None).expect("canvas should build");
    let mut renderer = AnnotationRenderer::new(canvas);
    if let Some(annotations) = &page.annotations {
        renderer.render_annotations_with_state_resolver(annotations, resolver)?;
    }
    drop(renderer);
    Ok(backend)
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

fn assert_color(actual: Color, expected: (f32, f32, f32)) {
    approx_eq(actual.r, expected.0);
    approx_eq(actual.g, expected.1);
    approx_eq(actual.b, expected.2);
}

#[test]
fn uses_normal_appearance_when_present() {
    let annotation =
        b"<< /Type /Annot /Subtype /FreeText /Rect [10 20 110 50] /AP << /N 5 0 R >> >>";
    let appearance = appearance_stream(b"1 0 0 rg 0 0 100 30 re f", b"<< >>");
    let pdf = build_pdf(annotation, vec![(5, appearance)]);

    let backend = render_first_page(&pdf);

    assert_eq!(backend.fill_count, 1);
    assert_eq!(backend.stroke_count, 0);
    assert_color(backend.fill_colors[0], (1.0, 0.0, 0.0));
}

#[test]
fn maps_appearance_bbox_into_annotation_rect() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 110 50] /AP << /N 5 0 R >> >>";
    let appearance = appearance_stream(b"0 0 100 30 re f", b"<< >>");
    let pdf = build_pdf(annotation, vec![(5, appearance)]);

    let backend = render_first_page(&pdf);
    let bounds = path_bounds(
        backend
            .last_fill_path
            .as_ref()
            .expect("appearance should fill a path"),
    );

    approx_eq(bounds.left, 10.0);
    approx_eq(bounds.right, 110.0);
    approx_eq(bounds.top, 50.0);
    approx_eq(bounds.bottom, 80.0);
}

#[test]
fn selects_subdictionary_appearance_by_state() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 110 50] /AS /On /AP << /N << /On 5 0 R /Off 6 0 R >> >> >>";
    let on_appearance = appearance_stream(b"0 1 0 rg 0 0 100 30 re f", b"<< >>");
    let off_appearance = appearance_stream(b"1 0 0 rg 0 0 100 30 re f", b"<< >>");
    let pdf = build_pdf(annotation, vec![(5, on_appearance), (6, off_appearance)]);

    let backend = render_first_page(&pdf);

    assert_eq!(backend.fill_count, 1);
    assert_color(backend.fill_colors[0], (0.0, 1.0, 0.0));
}

#[test]
fn skips_unmatched_subdictionary_appearance_state() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 110 50] /AS /Missing /AP << /N << /On 5 0 R >> >> >>";
    let appearance = appearance_stream(b"0 0 100 30 re f", b"<< >>");
    let pdf = build_pdf(annotation, vec![(5, appearance)]);

    let backend = render_first_page(&pdf);

    assert_eq!(backend.fill_count, 0);
    assert_eq!(backend.stroke_count, 0);
}

#[test]
fn uses_rollover_appearance_when_requested() {
    let annotation =
        b"<< /Type /Annot /Subtype /Widget /Rect [10 20 110 50] /AP << /N 5 0 R /R 6 0 R >> >>";
    let normal_appearance = appearance_stream(b"1 0 0 rg 0 0 100 30 re f", b"<< >>");
    let rollover_appearance = appearance_stream(b"0 1 0 rg 0 0 100 30 re f", b"<< >>");
    let pdf = build_pdf(
        annotation,
        vec![(5, normal_appearance), (6, rollover_appearance)],
    );

    let backend = render_first_page_with_state(&pdf, AnnotationInteractionState::Rollover);

    assert_eq!(backend.fill_count, 1);
    assert_eq!(backend.stroke_count, 0);
    assert_color(backend.fill_colors[0], (0.0, 1.0, 0.0));
}

#[test]
fn rollover_falls_back_to_normal_appearance_without_extra_cue() {
    let annotation = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 110 50] /AP << /N 5 0 R >> >>";
    let normal_appearance = appearance_stream(b"1 0 0 rg 0 0 100 30 re f", b"<< >>");
    let pdf = build_pdf(annotation, vec![(5, normal_appearance)]);

    let backend = render_first_page_with_state(&pdf, AnnotationInteractionState::Rollover);

    assert_eq!(backend.fill_count, 1);
    assert_eq!(backend.stroke_count, 0);
    assert_color(backend.fill_colors[0], (1.0, 0.0, 0.0));
}

#[test]
fn annotation_without_appearance_renders_nothing() {
    let annotation =
        b"<< /Type /Annot /Subtype /Highlight /Rect [10 60 60 80] /QuadPoints [10 80 60 80 10 60 60 60] >>";
    let pdf = build_pdf(annotation, Vec::new());

    let backend = render_first_page(&pdf);

    assert_eq!(backend.fill_count, 0);
    assert_eq!(backend.stroke_count, 0);
}

#[test]
fn annotation_without_rect_renders_nothing() {
    let annotation = b"<< /Type /Annot /Subtype /Popup /AP << /N 5 0 R >> >>";
    let appearance = appearance_stream(b"1 0 0 rg 0 0 100 30 re f", b"<< >>");
    let pdf = build_pdf(annotation, vec![(5, appearance)]);

    let backend = render_first_page(&pdf);

    assert_eq!(backend.fill_count, 0);
    assert_eq!(backend.stroke_count, 0);
}

#[test]
fn unknown_annotation_uses_appearance_when_present() {
    let annotation =
        b"<< /Type /Annot /Subtype /VendorThing /Rect [10 20 110 50] /AP << /N 5 0 R /R 6 0 R >> >>";
    let normal_appearance = appearance_stream(b"1 0 0 rg 0 0 100 30 re f", b"<< >>");
    let rollover_appearance = appearance_stream(b"0 1 0 rg 0 0 100 30 re f", b"<< >>");
    let pdf = build_pdf(
        annotation,
        vec![(5, normal_appearance), (6, rollover_appearance)],
    );

    let backend = render_first_page_with_state(&pdf, AnnotationInteractionState::Rollover);

    assert_eq!(backend.fill_count, 1);
    assert_eq!(backend.stroke_count, 0);
    assert_color(backend.fill_colors[0], (0.0, 1.0, 0.0));
}

#[test]
fn state_resolver_selects_appearance_per_annotation() {
    let first_annotation =
        b"<< /Type /Annot /Subtype /Widget /Rect [10 20 60 50] /AP << /N 6 0 R /R 7 0 R >> >>";
    let second_annotation =
        b"<< /Type /Annot /Subtype /Widget /Rect [70 20 120 50] /AP << /N 8 0 R /D 9 0 R >> >>";
    let first_normal = appearance_stream(b"1 0 0 rg 0 0 100 30 re f", b"<< >>");
    let first_rollover = appearance_stream(b"0 1 0 rg 0 0 100 30 re f", b"<< >>");
    let second_normal = appearance_stream(b"0 0 1 rg 0 0 100 30 re f", b"<< >>");
    let second_down = appearance_stream(b"0 0 0 rg 0 0 100 30 re f", b"<< >>");
    let pdf = build_pdf_with_annotations(
        vec![
            (4, first_annotation.to_vec()),
            (5, second_annotation.to_vec()),
        ],
        vec![
            (6, first_normal),
            (7, first_rollover),
            (8, second_normal),
            (9, second_down),
        ],
    );

    let backend = render_first_page_with_state_resolver(&pdf, |index, _annotation| {
        if index == 0 {
            AnnotationInteractionState::Rollover
        } else {
            AnnotationInteractionState::Down
        }
    });

    assert_eq!(backend.fill_count, 2);
    assert_color(backend.fill_colors[0], (0.0, 1.0, 0.0));
    assert_color(backend.fill_colors[1], (0.0, 0.0, 0.0));
}
