//! PDFium reference renderer: page images, page-space mapping, and extracted text.

use anyhow::{Context, Result, anyhow};
use image::RgbaImage;
use pdfium_render::prelude::{PdfColor, PdfPage, PdfRect, PdfRenderConfig, Pdfium};
use std::path::Path;

/// Loads PDFium from `library`.
pub fn load(library: &Path) -> Result<Pdfium> {
    let bindings = Pdfium::bind_to_library(library)
        .with_context(|| format!("loading PDFium from {}", library.display()))?;
    Ok(Pdfium::new(bindings))
}

/// Returns the rendered page size in points, with `/Rotate` applied.
pub fn page_size(page: &PdfPage<'_>) -> [f32; 2] {
    [page.width().value, page.height().value]
}

/// Content-only render settings matching `PdfRenderer::render`, which excludes annotations.
pub fn config(width: u32, height: u32) -> Result<PdfRenderConfig> {
    Ok(PdfRenderConfig::new()
        .set_target_size(i32::try_from(width)?, i32::try_from(height)?)
        .render_annotations(false)
        .render_form_data(false)
        .set_clear_color(PdfColor::WHITE))
}

/// Renders the page to an RGBA image with the given settings.
pub fn render(page: &PdfPage<'_>, config: &PdfRenderConfig) -> Result<RgbaImage> {
    let bitmap = page.render_with_config(config)?;
    let width = u32::try_from(bitmap.width())?;
    let height = u32::try_from(bitmap.height())?;
    RgbaImage::from_raw(width, height, bitmap.as_rgba_bytes())
        .ok_or_else(|| anyhow!("PDFium bitmap has an unexpected size"))
}

/// Maps a pixel box to PDF user space `[left, bottom, right, top]`.
pub fn page_space(
    page: &PdfPage<'_>,
    config: &PdfRenderConfig,
    pixels: [u32; 4],
) -> Option<[f32; 4]> {
    let [x0, y0, x1, y1] = pixels.map(|value| i32::try_from(value).ok());
    let (ax, ay) = page.pixels_to_points(x0?, y0?, config).ok()?;
    let (bx, by) = page.pixels_to_points(x1?, y1?, config).ok()?;
    Some([
        ax.value.min(bx.value),
        ay.value.min(by.value),
        ax.value.max(bx.value),
        ay.value.max(by.value),
    ])
}

/// Returns the text PDFium extracts inside a user-space box.
pub fn text_in(page: &PdfPage<'_>, bounds: [f32; 4]) -> String {
    let [left, bottom, right, top] = bounds;
    page.text()
        .map(|text| text.inside_rect(PdfRect::new_from_values(bottom, left, top, right)))
        .unwrap_or_default()
}
