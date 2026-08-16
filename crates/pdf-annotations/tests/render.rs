#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used)]

use std::sync::Arc;

use pdf_annotation_types::Annotation;
use pdf_annotations::{AnnotationAppearanceState, AnnotationRenderer};
use pdf_canvas::{
    canvas_backend::{CanvasBackend, Shader},
    pdf_canvas::PdfCanvas,
    recording_canvas::RecordingCanvas,
    stroke_style::StrokeStyle,
};
use pdf_document::reader::PdfReader;
use pdf_graphics::{
    BlendMode, Image, MaskMode, PathFillType, color::Color, pdf_path::PathVerb, pdf_path::PdfPath,
    rect::Rect, transform::Transform,
};

#[derive(Default)]
struct CountingCanvas {
    fill_count: usize,
    stroke_count: usize,
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
        _image: &Image,
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

fn render_with_resolver<F>(pdf: &[u8], resolver: F) -> CountingCanvas
where
    F: FnMut(&Annotation) -> AnnotationAppearanceState,
{
    let document = PdfReader
        .read_from_bytes(pdf, None)
        .expect("PDF should parse");
    let page = document.pages.first().expect("page should exist");
    let mut backend = CountingCanvas::default();
    let canvas = PdfCanvas::new(&mut backend, page, None).expect("canvas should build");
    let mut renderer = AnnotationRenderer::new(canvas);
    if let Some(annotations) = &page.annotations {
        renderer
            .render_all_with_state(annotations, resolver)
            .expect("annotations should render");
    }
    drop(renderer);
    backend
}

fn render(pdf: &[u8], state: AnnotationAppearanceState) -> CountingCanvas {
    render_with_resolver(pdf, |_| state)
}

fn build_pdf(annotation: &[u8], appearances: Vec<(usize, Vec<u8>)>) -> Vec<u8> {
    build_pdf_with_annotations(vec![(4, annotation.to_vec())], appearances)
}

fn build_pdf_with_annotations(
    annotations: Vec<(usize, Vec<u8>)>,
    appearances: Vec<(usize, Vec<u8>)>,
) -> Vec<u8> {
    let annotation_refs = annotations
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
    objects.extend(annotations);
    objects.extend(appearances);

    let max_object_number = objects.iter().map(|(number, _)| *number).max().unwrap_or(0);
    let mut offsets = vec![0; max_object_number + 1];
    let mut data = b"%PDF-1.7\n".to_vec();
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

fn appearance_stream(content: &[u8]) -> Vec<u8> {
    let mut stream = format!(
        "<< /Type /XObject /Subtype /Form /BBox [0 0 100 30] /Resources << >> /Length {} >>\nstream\n",
        content.len()
    )
    .into_bytes();
    stream.extend_from_slice(content);
    stream.extend_from_slice(b"\nendstream");
    stream
}

fn path_bounds(path: &PdfPath) -> Rect {
    let mut bounds = Rect {
        left: f32::INFINITY,
        top: f32::INFINITY,
        right: f32::NEG_INFINITY,
        bottom: f32::NEG_INFINITY,
    };
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
            bounds.left = bounds.left.min(*x);
            bounds.top = bounds.top.min(*y);
            bounds.right = bounds.right.max(*x);
            bounds.bottom = bounds.bottom.max(*y);
        }
    }
    bounds
}

fn assert_color(actual: Color, expected: (f32, f32, f32)) {
    assert!((actual.r - expected.0).abs() <= 0.001);
    assert!((actual.g - expected.1).abs() <= 0.001);
    assert!((actual.b - expected.2).abs() <= 0.001);
}

#[test]
fn renders_and_places_normal_appearance() {
    let annotation =
        b"<< /Type /Annot /Subtype /FreeText /Rect [10 20 110 50] /AP << /N 5 0 R >> >>";
    let pdf = build_pdf(
        annotation,
        vec![(5, appearance_stream(b"1 0 0 rg 0 0 100 30 re f"))],
    );

    let backend = render(&pdf, AnnotationAppearanceState::Normal);
    assert_eq!(backend.fill_count, 1);
    assert_color(backend.fill_colors[0], (1.0, 0.0, 0.0));
    let bounds = path_bounds(&backend.fill_paths[0]);
    assert_eq!(bounds.left, 10.0);
    assert_eq!(bounds.right, 110.0);
    assert_eq!(bounds.top, 50.0);
    assert_eq!(bounds.bottom, 80.0);
}

#[test]
fn selects_named_and_rollover_appearances_with_fallback() {
    let named = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 110 50] /AS /On /AP << /N << /On 5 0 R /Off 6 0 R >> >> >>";
    let pdf = build_pdf(
        named,
        vec![
            (5, appearance_stream(b"0 1 0 rg 0 0 100 30 re f")),
            (6, appearance_stream(b"1 0 0 rg 0 0 100 30 re f")),
        ],
    );
    assert_color(
        render(&pdf, AnnotationAppearanceState::Normal).fill_colors[0],
        (0.0, 1.0, 0.0),
    );

    let rollover =
        b"<< /Type /Annot /Subtype /Widget /Rect [10 20 110 50] /AP << /N 5 0 R /R 6 0 R >> >>";
    let pdf = build_pdf(
        rollover,
        vec![
            (5, appearance_stream(b"1 0 0 rg 0 0 100 30 re f")),
            (6, appearance_stream(b"0 1 0 rg 0 0 100 30 re f")),
        ],
    );
    assert_color(
        render(&pdf, AnnotationAppearanceState::Rollover).fill_colors[0],
        (0.0, 1.0, 0.0),
    );

    let fallback = b"<< /Type /Annot /Subtype /Widget /Rect [10 20 110 50] /AP << /N 5 0 R >> >>";
    let pdf = build_pdf(
        fallback,
        vec![(5, appearance_stream(b"1 0 0 rg 0 0 100 30 re f"))],
    );
    assert_color(
        render(&pdf, AnnotationAppearanceState::Rollover).fill_colors[0],
        (1.0, 0.0, 0.0),
    );
}

#[test]
fn skips_annotations_without_usable_appearance_or_rect() {
    let cases: &[&[u8]] = &[
        b"<< /Type /Annot /Subtype /Highlight /Rect [10 60 60 80] /QuadPoints [10 80 60 80 10 60 60 60] >>",
        b"<< /Type /Annot /Subtype /Popup /AP << /N 5 0 R >> >>",
        b"<< /Type /Annot /Subtype /Widget /Rect [10 20 110 50] /AS /Missing /AP << /N << /On 5 0 R >> >> >>",
    ];
    for annotation in cases {
        let pdf = build_pdf(annotation, vec![(5, appearance_stream(b"0 0 100 30 re f"))]);
        let backend = render(&pdf, AnnotationAppearanceState::Normal);
        assert_eq!(backend.fill_count, 0);
        assert_eq!(backend.stroke_count, 0);
    }
}

#[test]
fn renders_unknown_annotations_and_resolves_state_per_annotation() {
    let unknown = b"<< /Type /Annot /Subtype /VendorThing /Rect [10 20 110 50] /AP << /N 5 0 R /R 6 0 R >> >>";
    let pdf = build_pdf(
        unknown,
        vec![
            (5, appearance_stream(b"1 0 0 rg 0 0 100 30 re f")),
            (6, appearance_stream(b"0 1 0 rg 0 0 100 30 re f")),
        ],
    );
    assert_color(
        render(&pdf, AnnotationAppearanceState::Rollover).fill_colors[0],
        (0.0, 1.0, 0.0),
    );

    let first =
        b"<< /Type /Annot /Subtype /Widget /Rect [10 20 60 50] /AP << /N 6 0 R /R 7 0 R >> >>";
    let second =
        b"<< /Type /Annot /Subtype /Widget /Rect [70 20 120 50] /AP << /N 8 0 R /D 9 0 R >> >>";
    let pdf = build_pdf_with_annotations(
        vec![(4, first.to_vec()), (5, second.to_vec())],
        vec![
            (6, appearance_stream(b"1 0 0 rg 0 0 100 30 re f")),
            (7, appearance_stream(b"0 1 0 rg 0 0 100 30 re f")),
            (8, appearance_stream(b"0 0 1 rg 0 0 100 30 re f")),
            (9, appearance_stream(b"0 0 0 rg 0 0 100 30 re f")),
        ],
    );
    let backend = render_with_resolver(&pdf, |annotation| {
        if annotation.id().get() == 0 {
            AnnotationAppearanceState::Rollover
        } else {
            AnnotationAppearanceState::Down
        }
    });
    assert_eq!(backend.fill_count, 2);
    assert_color(backend.fill_colors[0], (0.0, 1.0, 0.0));
    assert_color(backend.fill_colors[1], (0.0, 0.0, 0.0));
}
