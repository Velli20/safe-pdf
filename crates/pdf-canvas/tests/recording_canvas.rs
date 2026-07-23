#![allow(clippy::expect_used)]

mod common;

use common::replay;
use pdf_canvas::{
    canvas_backend::CanvasBackend, recording_canvas::RecordingCanvas, stroke_style::StrokeStyle,
};
use pdf_graphics::{DashPattern, color::Color, pdf_path::PdfPath};

#[test]
fn replay_preserves_stroke_style() {
    let mut recording = RecordingCanvas::new(100.0, 100.0);
    let mut path = PdfPath::default();
    path.move_to(0.0, 0.0);
    path.line_to(10.0, 0.0);
    let stroke_style = StrokeStyle {
        dash_pattern: Some(DashPattern {
            intervals: vec![4.0, 2.0],
            phase: 1.0,
        }),
    };

    recording
        .stroke_path(
            &path,
            Color::from_rgb(0.0, 0.0, 0.0),
            1.0,
            &stroke_style,
            &None,
            None,
        )
        .expect("stroke should record");

    let observer = replay(&recording);

    assert_eq!(observer.stroke_styles, vec![stroke_style]);
}
