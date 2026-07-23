#![allow(clippy::expect_used)]

mod common;

use common::{content_stream, replay};
use pdf_canvas::{error::PdfCanvasError, pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_color_space::error::ColorSpaceError;
use pdf_document::page::PdfPage;
use pdf_graphics::color::Color;

fn render(data: &[u8]) -> Result<RecordingCanvas, PdfCanvasError> {
    let stream = content_stream(1, data);
    let page = PdfPage::default();
    let mut recording = RecordingCanvas::new(100.0, 100.0);
    {
        let mut canvas = PdfCanvas::new(&mut recording, &page, None)?;
        canvas.render_content_stream(&stream, None, None, None, None)?;
    }
    Ok(recording)
}

#[test]
fn default_device_gray_scn_with_three_components_falls_back_to_rgb() {
    let recording =
        render(b"0.294118 0.019608 0.196078 scn 0 0 1 1 re f").expect("RGB fallback should render");

    let observer = replay(&recording);
    assert_eq!(
        observer.fill_colors,
        vec![Color::from_rgb(0.294_118, 0.019_608, 0.196_078)]
    );
}

#[test]
fn device_rgb_scn_with_three_components_still_uses_active_space() {
    let recording =
        render(b"/DeviceRGB cs 0 0 0 scn 0 0 1 1 re f").expect("RGB color should render");

    let observer = replay(&recording);
    assert_eq!(observer.fill_colors, vec![Color::from_rgb(0.0, 0.0, 0.0)]);
}

#[test]
fn invalid_generic_operand_count_still_returns_component_error() {
    assert!(matches!(
        render(b"0.2 0.4 scn"),
        Err(PdfCanvasError::ColorSpaceError(
            ColorSpaceError::InsufficientComponents(1, 2)
        ))
    ));
}
