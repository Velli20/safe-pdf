//! Safe-PDF rendering to a Skia raster, and error formatting with captured traces.

use crate::{model::ErrorDetail, trace_backend::TraceBackend};
use anyhow::{Result, anyhow, bail};
use image::RgbaImage;
use pdf_graphics_skia::skia_canvas_backend::SkiaCanvasBackend;
use pdf_renderer::{PdfRenderer, text_selection::PageTextLayout};

/// Renders page content (no annotations) onto a white raster of `width` x `height` pixels.
pub fn render(renderer: &PdfRenderer, page: usize, width: u32, height: u32) -> Result<RgbaImage> {
    let (w, h) = (i32::try_from(width)?, i32::try_from(height)?);
    let mut surface = skia_safe::surfaces::raster_n32_premul((w, h))
        .ok_or_else(|| anyhow!("cannot allocate Skia raster surface"))?;
    surface.canvas().clear(skia_safe::Color::WHITE);
    {
        let mut backend = SkiaCanvasBackend {
            surface: &mut surface,
            width: f32::from(u16::try_from(width)?),
            height: f32::from(u16::try_from(height)?),
        };
        renderer.render(&mut backend, page)?;
    }
    let row_bytes = usize::try_from(width)?
        .checked_mul(4)
        .ok_or_else(|| anyhow!("row size overflow"))?;
    let mut pixels = vec![
        0;
        row_bytes
            .checked_mul(usize::try_from(height)?)
            .ok_or_else(|| anyhow!("image size overflow"))?
    ];
    let info = skia_safe::ImageInfo::new(
        (w, h),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Unpremul,
        None,
    );
    if !surface.image_snapshot().read_pixels(
        &info,
        &mut pixels,
        row_bytes,
        (0, 0),
        skia_safe::image::CachingHint::Disallow,
    ) {
        bail!("cannot read Skia raster pixels");
    }
    RgbaImage::from_raw(width, height, pixels).ok_or_else(|| anyhow!("cannot build Safe-PDF image"))
}

/// Replays the page onto a bounds-only backend, returning the draw log and text layout.
pub fn trace(
    renderer: &PdfRenderer,
    page: usize,
    width: u32,
    height: u32,
) -> Result<(TraceBackend, PageTextLayout)> {
    let mut backend = TraceBackend::new(
        f32::from(u16::try_from(width)?),
        f32::from(u16::try_from(height)?),
    );
    let layout = renderer.render_with_text_layout(&mut backend, page)?;
    Ok((backend, layout))
}

/// Formats an error chain with the conversion trace nearest to its origin.
///
/// Errors constructed without a traced conversion have no backtrace; a trace captured
/// here would only show the harness.
pub fn error_detail(error: &anyhow::Error) -> ErrorDetail {
    let trace = error.chain().find_map(|cause| {
        cause
            .downcast_ref::<pdf_document::error::PdfReaderError>()
            .and_then(pdf_document::error::PdfReaderError::trace)
            .or_else(|| {
                cause
                    .downcast_ref::<pdf_renderer::PdfRendererError>()
                    .and_then(pdf_renderer::PdfRendererError::trace)
            })
            .cloned()
    });
    ErrorDetail {
        message: error.to_string(),
        chain: error.chain().skip(1).map(ToString::to_string).collect(),
        backtrace: trace.and_then(|trace| trace.full_backtrace()),
    }
}
