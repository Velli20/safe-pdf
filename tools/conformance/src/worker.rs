//! Worker entry points. Each runs in its own process and prints one JSON value on stdout.

use crate::{
    attribution, compare, golden, inventory,
    model::{Diagnostic, PageOutput, ReadOutput},
    process::STAGE_MARKER,
    reference, regions, safe_render,
};
use anyhow::{Result, anyhow};
use image::{DynamicImage, GenericImage, Rgba, RgbaImage, imageops::FilterType};
use num_traits::ToPrimitive;
use pdf_canvas::PageViewport;
use pdf_document::{document::PdfDocument, reader::PdfReader};
use pdf_graphics::point::Point;
use pdf_renderer::PdfRenderer;
use std::{fmt::Write as _, fs, path::Path};

/// Regions reported per page.
const REGION_LIMIT: usize = 6;
/// Regions that get zoomed crops.
const CROP_LIMIT: usize = 3;
/// Longest side of the combined comparison image, sized for multimodal models.
const COMPARE_SIDE: u32 = 1568;
/// Content stream operators written to the per-page listing.
const OPERATOR_LIMIT: usize = 4000;

/// Settings of a page worker.
pub struct PageJob<'a> {
    /// PDF file.
    pub pdf: &'a Path,
    /// Document password.
    pub password: Option<&'a str>,
    /// Zero-based page index.
    pub page: usize,
    /// Pixels per PDF point; golden references fix their own size.
    pub scale: f32,
    /// Largest raster side in pixels.
    pub max_side: u32,
    /// Mismatch fraction up to which no images are written.
    pub tolerance: f64,
    /// Case directory receiving images and listings.
    pub out_dir: &'a Path,
    /// PDFium shared library; `None` compares against the corpus's golden images.
    pub pdfium: Option<&'a Path>,
}

fn stage(name: &str) {
    eprintln!("{STAGE_MARKER}{name}");
}

fn read_safe(bytes: &[u8], password: Option<&str>) -> Result<pdf_document::report::PdfReadReport> {
    Ok(PdfReader.read_with_report(bytes, password.map(str::as_bytes))?)
}

/// Reads the document with Safe-PDF and counts the reference's pages.
pub fn read(pdf: &Path, password: Option<&str>, pdfium: Option<&Path>) -> Result<()> {
    let bytes = fs::read(pdf)?;
    let mut output = ReadOutput::default();
    stage("safe-read");
    match read_safe(&bytes, password) {
        Ok(report) => {
            output.diagnostics = report
                .diagnostics()
                .iter()
                .map(|diagnostic| Diagnostic {
                    kind: format!("{:?}", diagnostic.kind),
                    page: diagnostic.page,
                    object: diagnostic
                        .object
                        .map(|id| format!("{} {}", id.number, id.generation)),
                    byte_offset: diagnostic.byte_offset,
                    message: diagnostic.message.clone(),
                    backtrace: diagnostic.trace.full_backtrace(),
                })
                .collect();
            let document = report.document();
            output.safe_pages = Some(document.pages.len());
            stage("inventory");
            if let Some(state) = document
                .pages
                .iter()
                .find_map(|page| page.read_state.as_ref())
            {
                output.inventory = inventory::scan(state.source());
            }
        }
        Err(error) => output.safe_error = Some(safe_render::error_detail(&error)),
    }
    stage("reference-open");
    match pdfium {
        Some(library) => {
            let pdfium = reference::load(library)?;
            match pdfium.load_pdf_from_byte_slice(&bytes, password) {
                Ok(document) => {
                    output.reference_pages = usize::try_from(document.pages().len()).ok();
                }
                Err(error) => output.reference_error = Some(error.to_string()),
            }
        }
        None => match golden::page_count(pdf) {
            0 => {
                output.reference_error =
                    Some("the corpus has no golden images for this PDF".to_owned())
            }
            pages => output.reference_pages = Some(pages),
        },
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

/// Loads the corpus golden of a page.
fn load_golden(job: &PageJob<'_>) -> Result<RgbaImage, String> {
    let path = golden::find(job.pdf, job.page)
        .ok_or_else(|| format!("the corpus has no golden image for page {}", job.page))?;
    image::open(&path)
        .map(|image| image.to_rgba8())
        .map_err(|error| format!("{}: {error}", path.display()))
}

/// Maps a pixel box to PDF user space through Safe-PDF's own page viewport.
fn viewport_box(viewport: &PageViewport, [x0, y0, x1, y1]: [u32; 4]) -> Option<[f32; 4]> {
    let a = viewport
        .map_device_point(Point {
            x: x0.to_f32()?,
            y: y0.to_f32()?,
        })
        .ok()?;
    let b = viewport
        .map_device_point(Point {
            x: x1.to_f32()?,
            y: y1.to_f32()?,
        })
        .ok()?;
    Some([a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)])
}

/// Returns device-pixel boxes of annotations drawn in golden images, grown by two pixels
/// for anti-aliasing. Links and popups have no appearance in `pdfium_test` output.
fn annotation_boxes(page: &pdf_document::page::PdfPage, width: u32, height: u32) -> Vec<[u32; 4]> {
    let Some(device) = width.to_f32().zip(height.to_f32()) else {
        return Vec::new();
    };
    let Ok(viewport) = PageViewport::from_page(page, None, [device.0, device.1]) else {
        return Vec::new();
    };
    page.annotations
        .iter()
        .flatten()
        .filter(|annotation| annotation.subtype != b"Link" && annotation.subtype != b"Popup")
        .filter_map(|annotation| {
            let rect = annotation.rect.as_ref()?;
            let rect = pdf_graphics::rect::Rect {
                left: rect.left.to_f32()?,
                top: rect.top.to_f32()?,
                right: rect.right.to_f32()?,
                bottom: rect.bottom.to_f32()?,
            };
            let mapped = viewport.map_rect(&rect).ok()?.normalized();
            let clamp = |value: f32, limit: u32| value.max(0.0).to_u32().map(|v| v.min(limit));
            Some([
                clamp((mapped.left - 2.0).floor(), width)?,
                clamp((mapped.top - 2.0).floor(), height)?,
                clamp((mapped.right + 2.0).ceil(), width)?,
                clamp((mapped.bottom + 2.0).ceil(), height)?,
            ])
        })
        .collect()
}

/// Renders one page with Safe-PDF, compares it with the reference, and writes evidence files.
pub fn page(job: &PageJob<'_>) -> Result<()> {
    let bytes = fs::read(job.pdf)?;
    let mut output = PageOutput {
        page: job.page,
        ..PageOutput::default()
    };
    stage("safe-read");
    let safe_document = read_safe(&bytes, job.password).map(|report| report.into_document());
    let safe_points = match &safe_document {
        Ok(document) => document
            .get_page(job.page)
            .ok_or_else(|| anyhow!("page {} not found", job.page))
            .and_then(|page| Ok(PageViewport::page_size(page)?)),
        Err(_) => Err(anyhow!("document not loaded")),
    };

    stage("reference-open");
    let pdfium = job.pdfium.map(reference::load).transpose()?;
    let reference_document = pdfium.as_ref().map(|pdfium| {
        pdfium
            .load_pdf_from_byte_slice(&bytes, job.password)
            .map_err(|error| error.to_string())
    });
    let reference_page = reference_document.as_ref().map(|document| {
        document
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|document| {
                let index = i32::try_from(job.page).map_err(|error| error.to_string())?;
                document
                    .pages()
                    .get(index)
                    .map_err(|error| error.to_string())
            })
    });
    let golden = job.pdfium.is_none().then(|| load_golden(job));

    let size = match (&golden, &reference_page) {
        (Some(Ok(image)), _) => Ok(image.dimensions()),
        _ => safe_points
            .or_else(|error| match &reference_page {
                Some(Ok(page)) => Ok(reference::page_size(page)),
                _ => Err(error),
            })
            .and_then(|points| raster_size(points, job.scale, job.max_side)),
    };
    let (width, height) = match size {
        Ok(size) => size,
        Err(error) => {
            output.safe_error = Some(safe_render::error_detail(&error));
            output.reference_error = match (golden, reference_page) {
                (Some(Err(error)), _) | (_, Some(Err(error))) => Some(error),
                _ => None,
            };
            println!("{}", serde_json::to_string(&output)?);
            return Ok(());
        }
    };
    output.size = [width, height];

    stage("reference-render");
    let config = reference::config(width, height)?;
    let reference_image = match (golden, &reference_page) {
        (Some(golden), _) => golden,
        (None, Some(Ok(page))) => reference::render(page, &config)
            .map_err(|error| error.to_string())
            .and_then(|image| {
                if image.dimensions() == (width, height) {
                    Ok(image)
                } else {
                    Err(format!(
                        "PDFium rendered {:?}, expected {:?}",
                        image.dimensions(),
                        (width, height)
                    ))
                }
            }),
        (None, Some(Err(error))) => Err(error.clone()),
        (None, None) => Err("no reference renderer".to_owned()),
    };

    stage("safe-render");
    let renderer = safe_document.map(PdfRenderer::new);
    let safe_image = renderer
        .as_ref()
        .map_err(|error| anyhow!("{error}"))
        .and_then(|renderer| safe_render::render(renderer, job.page, width, height));

    let prefix = format!("p{}", job.page);
    match (&reference_image, &safe_image) {
        (Ok(reference_image), Ok(safe_image)) => {
            stage("compare");
            // Golden images draw annotations and form fields; Safe-PDF's render does not.
            let ignore = if job.pdfium.is_none() {
                renderer
                    .as_ref()
                    .ok()
                    .and_then(|renderer| renderer.document().get_page(job.page))
                    .map(|page| annotation_boxes(page, width, height))
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let comparison = compare::compare(reference_image, safe_image, &ignore)?;
            let failed = comparison.metrics.mismatch > job.tolerance;
            output.metrics = Some(comparison.metrics);
            if failed {
                stage("safe-trace");
                let traced = renderer.as_ref().ok().and_then(|renderer| {
                    safe_render::trace(renderer, job.page, width, height).ok()
                });
                output.draw_count = traced
                    .as_ref()
                    .map_or(0, |(backend, _)| backend.draws.len());
                stage("regions");
                let raw =
                    regions::find(&comparison.mask, reference_image, safe_image, REGION_LIMIT);
                let viewport = renderer
                    .as_ref()
                    .ok()
                    .and_then(|renderer| renderer.document().get_page(job.page))
                    .and_then(|page| {
                        let device = [width.to_f32()?, height.to_f32()?];
                        PageViewport::from_page(page, None, device).ok()
                    });
                let reference_page = reference_page.as_ref().and_then(|page| page.as_ref().ok());
                let page_space = |pixels: [u32; 4]| match reference_page {
                    Some(page) => reference::page_space(page, &config, pixels),
                    None => viewport
                        .as_ref()
                        .and_then(|viewport| viewport_box(viewport, pixels)),
                };
                let reference_text = |bounds: [f32; 4]| {
                    reference_page.map_or_else(String::new, |page| reference::text_in(page, bounds))
                };
                output.regions = attribution::attribute(
                    raw,
                    &page_space,
                    &reference_text,
                    traced.as_ref().map_or(&[], |(backend, _)| &backend.draws),
                    traced.as_ref().map(|(_, layout)| layout),
                );
                stage("write");
                write_png(
                    job,
                    &mut output,
                    &format!("{prefix}-ref.png"),
                    reference_image,
                )?;
                write_png(job, &mut output, &format!("{prefix}-safe.png"), safe_image)?;
                write_png(
                    job,
                    &mut output,
                    &format!("{prefix}-diff.png"),
                    &comparison.diff,
                )?;
                let combined = side_by_side(&[reference_image, safe_image, &comparison.diff], 1.0)?;
                let combined = fit(combined, COMPARE_SIDE);
                write_png(
                    job,
                    &mut output,
                    &format!("{prefix}-compare.png"),
                    &combined,
                )?;
                for (rank, region) in output.regions.clone().iter().enumerate().take(CROP_LIMIT) {
                    let crop = region_crop(reference_image, safe_image, region.pixels)?;
                    write_png(
                        job,
                        &mut output,
                        &format!("{prefix}-region{rank}.png"),
                        &crop,
                    )?;
                }
                if let Ok(renderer) = &renderer {
                    write_operators(job, &mut output, renderer.document(), &prefix)?;
                }
            }
        }
        (reference_result, safe_result) => {
            if let Err(error) = reference_result {
                output.reference_error = Some(error.clone());
            }
            if let Err(error) = safe_result {
                output.safe_error = Some(safe_render::error_detail(error));
                if let Ok(image) = reference_result {
                    write_png(job, &mut output, &format!("{prefix}-ref.png"), image)?;
                }
                if let Ok(renderer) = &renderer {
                    write_operators(job, &mut output, renderer.document(), &prefix)?;
                }
            }
        }
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

/// Returns the raster size for a page, shrinking the scale if a side would exceed `max_side`.
fn raster_size([width, height]: [f32; 2], scale: f32, max_side: u32) -> Result<(u32, u32)> {
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return Err(anyhow!("invalid page size {width} x {height}"));
    }
    let limit = max_side
        .to_f32()
        .ok_or_else(|| anyhow!("invalid size limit"))?;
    let scale = scale.min(limit / width.max(height));
    let side = |points: f32| {
        (points * scale)
            .ceil()
            .max(1.0)
            .to_u32()
            .ok_or_else(|| anyhow!("page size {points} is not representable"))
    };
    Ok((side(width)?, side(height)?))
}

fn write_png(
    job: &PageJob<'_>,
    output: &mut PageOutput,
    name: &str,
    image: &RgbaImage,
) -> Result<()> {
    fs::create_dir_all(job.out_dir)?;
    image.save(job.out_dir.join(name))?;
    output.files.push(name.to_owned());
    Ok(())
}

/// Places images left to right on a gray background with a gap.
fn side_by_side(images: &[&RgbaImage], scale: f32) -> Result<RgbaImage> {
    const GAP: u32 = 8;
    let scaled: Vec<RgbaImage> = images
        .iter()
        .map(|image| {
            let width = (image.width().to_f32().unwrap_or(1.0) * scale)
                .round()
                .to_u32()
                .unwrap_or(1);
            let height = (image.height().to_f32().unwrap_or(1.0) * scale)
                .round()
                .to_u32()
                .unwrap_or(1);
            if scale == 1.0 {
                (*image).clone()
            } else {
                image::imageops::resize(*image, width.max(1), height.max(1), FilterType::Nearest)
            }
        })
        .collect();
    let width = scaled
        .iter()
        .map(RgbaImage::width)
        .fold(0u32, |sum, w| sum.saturating_add(w).saturating_add(GAP))
        .saturating_sub(GAP);
    let height = scaled.iter().map(RgbaImage::height).max().unwrap_or(1);
    let mut canvas = RgbaImage::from_pixel(width.max(1), height.max(1), Rgba([128, 128, 128, 255]));
    let mut x = 0u32;
    for image in &scaled {
        canvas.copy_from(image, x, 0)?;
        x = x.saturating_add(image.width()).saturating_add(GAP);
    }
    Ok(canvas)
}

fn fit(image: RgbaImage, side: u32) -> RgbaImage {
    if image.width().max(image.height()) <= side {
        return image;
    }
    DynamicImage::ImageRgba8(image)
        .resize(side, side, FilterType::Triangle)
        .to_rgba8()
}

/// Returns reference | Safe-PDF crops of a region with margin, enlarged when small.
fn region_crop(
    reference: &RgbaImage,
    safe: &RgbaImage,
    [x0, y0, x1, y1]: [u32; 4],
) -> Result<RgbaImage> {
    const MARGIN: u32 = 12;
    let left = x0.saturating_sub(MARGIN);
    let top = y0.saturating_sub(MARGIN);
    let right = x1.saturating_add(MARGIN).min(reference.width());
    let bottom = y1.saturating_add(MARGIN).min(reference.height());
    let (width, height) = (
        right.saturating_sub(left).max(1),
        bottom.saturating_sub(top).max(1),
    );
    let a = image::imageops::crop_imm(reference, left, top, width, height).to_image();
    let b = image::imageops::crop_imm(safe, left, top, width, height).to_image();
    let scale = if width.max(height) < 300 {
        3.0
    } else if width.max(height) < 600 {
        2.0
    } else {
        1.0
    };
    Ok(fit(side_by_side(&[&a, &b], scale)?, COMPARE_SIDE))
}

/// Writes the page's top-level content stream operators, numbered in execution order.
fn write_operators(
    job: &PageJob<'_>,
    output: &mut PageOutput,
    document: &PdfDocument,
    prefix: &str,
) -> Result<()> {
    let Some(content) = document
        .get_page(job.page)
        .and_then(|page| page.contents.as_ref())
    else {
        return Ok(());
    };
    let mut text = format!(
        "# Page {} content stream: {} operators (Form XObjects, patterns and Type3 glyphs are not expanded)\n",
        job.page,
        content.operators.len()
    );
    for (index, operator) in content.operators.iter().enumerate().take(OPERATOR_LIMIT) {
        let mut line = format!("{operator:?}");
        if line.len() > 240 {
            let cut = (0..=240)
                .rev()
                .find(|i| line.is_char_boundary(*i))
                .unwrap_or(0);
            line.truncate(cut);
            line.push('…');
        }
        writeln!(text, "{index:5} {line}")?;
    }
    if content.operators.len() > OPERATOR_LIMIT {
        writeln!(
            text,
            "… {} more",
            content.operators.len().saturating_sub(OPERATOR_LIMIT)
        )?;
    }
    let name = format!("{prefix}-content.txt");
    fs::create_dir_all(job.out_dir)?;
    fs::write(job.out_dir.join(&name), text)?;
    output.files.push(name);
    Ok(())
}
