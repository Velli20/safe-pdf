//! Anti-aliasing tolerant pixel comparison following the pixelmatch algorithm.

use crate::model::Metrics;
use anyhow::{Result, bail};
use image::{Rgba, RgbaImage};
use num_traits::ToPrimitive;

/// Per-pixel comparison outcome stored in [`Comparison::mask`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PixelDiff {
    /// Equal within tolerance, or an anti-aliasing difference.
    Same,
    /// The reference has ink where Safe-PDF shows paper.
    Missing,
    /// Safe-PDF has ink where the reference shows paper.
    Extra,
    /// Both show ink (or both paper) in different hues.
    Color,
    /// Both show ink of the same hue at different intensity: glyph or edge shapes differ.
    Shape,
}

/// Comparison of a Safe-PDF render against the reference.
pub struct Comparison {
    /// Summary scores.
    pub metrics: Metrics,
    /// Visualization: faded reference, red missing, blue extra, magenta color, yellow
    /// anti-aliasing, pale blue hatching for ignored (annotation) areas.
    pub diff: RgbaImage,
    /// Row-major per-pixel outcome.
    pub mask: Vec<PixelDiff>,
}

/// Squared YIQ distance for pixelmatch's default threshold of 0.1.
const MAX_DELTA: f64 = 35215.0 * 0.1 * 0.1;

/// Luma below which a pixel counts as ink.
const INK_LUMA: f64 = 200.0;

/// Compares two images of equal size. Pixels inside `ignore` boxes (`[x0, y0, x1, y1]`,
/// exclusive) are not compared and do not count as ink.
pub fn compare(reference: &RgbaImage, safe: &RgbaImage, ignore: &[[u32; 4]]) -> Result<Comparison> {
    if reference.dimensions() != safe.dimensions() {
        bail!(
            "reference is {:?} but Safe-PDF image is {:?}",
            reference.dimensions(),
            safe.dimensions()
        );
    }
    let (width, height) = reference.dimensions();
    let mut diff = RgbaImage::new(width, height);
    let mut mask = Vec::with_capacity(pixel_count(width, height));
    let (mut mismatched, mut total_abs, mut reference_ink, mut safe_ink) = (0u64, 0u64, 0u64, 0u64);
    let mut ignored = 0u64;
    for y in 0..height {
        for x in 0..width {
            if ignore
                .iter()
                .any(|&[x0, y0, x1, y1]| (x0..x1).contains(&x) && (y0..y1).contains(&y))
            {
                ignored = ignored.saturating_add(1);
                mask.push(PixelDiff::Same);
                let hatch = (x.wrapping_add(y) / 4) % 2 == 0;
                diff.put_pixel(
                    x,
                    y,
                    if hatch {
                        Rgba([205, 220, 240, 255])
                    } else {
                        Rgba([235, 242, 250, 255])
                    },
                );
                continue;
            }
            let a = pixel(reference, x, y);
            let b = pixel(safe, x, y);
            total_abs = total_abs.saturating_add(
                (0..3)
                    .map(|i| u64::from(channel(a, i).abs_diff(channel(b, i))))
                    .sum::<u64>(),
            );
            let luma_a = luma(a);
            let luma_b = luma(b);
            reference_ink = reference_ink.saturating_add(u64::from(luma_a < INK_LUMA));
            safe_ink = safe_ink.saturating_add(u64::from(luma_b < INK_LUMA));
            let delta = color_delta(a, b);
            let (outcome, color) = if delta <= MAX_DELTA {
                (PixelDiff::Same, faded(luma_a))
            } else if antialiased(reference, x, y, safe)
                || antialiased(safe, x, y, reference)
                || displaced(reference, safe, x, y)
            {
                (PixelDiff::Same, Rgba([255, 220, 0, 255]))
            } else if luma_a < INK_LUMA && luma_b >= INK_LUMA {
                (PixelDiff::Missing, Rgba([230, 0, 0, 255]))
            } else if luma_b < INK_LUMA && luma_a >= INK_LUMA {
                (PixelDiff::Extra, Rgba([0, 90, 255, 255]))
            } else if chroma_delta(a, b) > MIN_CHROMA_DELTA {
                (PixelDiff::Color, Rgba([200, 0, 200, 255]))
            } else {
                (PixelDiff::Shape, Rgba([255, 140, 0, 255]))
            };
            if outcome != PixelDiff::Same {
                mismatched = mismatched.saturating_add(1);
            }
            mask.push(outcome);
            diff.put_pixel(x, y, color);
        }
    }
    let count = f64::from(width) * f64::from(height);
    let ratio = |value: u64| to_f64(value) / count.max(1.0);
    Ok(Comparison {
        metrics: Metrics {
            mismatch: ratio(mismatched),
            mean_abs_rgb: ratio(total_abs) / 3.0,
            reference_ink: ratio(reference_ink),
            safe_ink: ratio(safe_ink),
            ignored: ratio(ignored),
        },
        diff,
        mask,
    })
}

/// Converts a count to `f64`; counts here stay far below 2^53.
pub fn to_f64(value: u64) -> f64 {
    value.to_f64().unwrap_or(f64::MAX)
}

/// Returns the number of pixels of an image.
pub fn pixel_count(width: u32, height: u32) -> usize {
    usize::try_from(u64::from(width).saturating_mul(u64::from(height))).unwrap_or(usize::MAX)
}

/// Returns a pixel, or white outside the image.
pub fn pixel(image: &RgbaImage, x: u32, y: u32) -> Rgba<u8> {
    image
        .get_pixel_checked(x, y)
        .copied()
        .unwrap_or(Rgba([255, 255, 255, 255]))
}

fn channel(pixel: Rgba<u8>, index: usize) -> u8 {
    pixel.0.get(index).copied().unwrap_or(255)
}

/// Returns the RGB channels blended over white.
fn rgb(pixel: Rgba<u8>) -> [f64; 3] {
    let alpha = f64::from(channel(pixel, 3)) / 255.0;
    [0, 1, 2].map(|i| 255.0 + (f64::from(channel(pixel, i)) - 255.0) * alpha)
}

/// Returns the YIQ luma (0 to 255) of a pixel blended over white.
pub fn luma(pixel: Rgba<u8>) -> f64 {
    let [r, g, b] = rgb(pixel);
    r * 0.298_895_31 + g * 0.586_622_47 + b * 0.114_482_23
}

/// Chroma distance below which two differing pixels count as the same hue.
const MIN_CHROMA_DELTA: f64 = 24.0;

/// Returns the distance between the chroma (YIQ I and Q) of two pixels.
fn chroma_delta(a: Rgba<u8>, b: Rgba<u8>) -> f64 {
    let chroma = |pixel: Rgba<u8>| {
        let [r, g, b] = rgb(pixel);
        (
            r * 0.595_977_99 - g * 0.274_176_10 - b * 0.321_801_89,
            r * 0.211_470_17 - g * 0.522_617_11 + b * 0.311_146_94,
        )
    };
    let (i1, q1) = chroma(a);
    let (i2, q2) = chroma(b);
    (i1 - i2).hypot(q1 - q2)
}

fn color_delta(a: Rgba<u8>, b: Rgba<u8>) -> f64 {
    let [r1, g1, b1] = rgb(a);
    let [r2, g2, b2] = rgb(b);
    let y = luma(a) - luma(b);
    let i = (r1 * 0.595_977_99 - g1 * 0.274_176_10 - b1 * 0.321_801_89)
        - (r2 * 0.595_977_99 - g2 * 0.274_176_10 - b2 * 0.321_801_89);
    let q = (r1 * 0.211_470_17 - g1 * 0.522_617_11 + b1 * 0.311_146_94)
        - (r2 * 0.211_470_17 - g2 * 0.522_617_11 + b2 * 0.311_146_94);
    0.5053 * y * y + 0.299 * i * i + 0.1957 * q * q
}

fn faded(luma: f64) -> Rgba<u8> {
    let value = (255.0 + (luma - 255.0) * 0.1)
        .round()
        .to_u8()
        .unwrap_or(255);
    Rgba([value, value, value, 255])
}

/// Returns the 3x3 neighbourhood of `(x, y)` clamped to the image, excluding the centre.
fn neighbours(image: &RgbaImage, x: u32, y: u32) -> impl Iterator<Item = (u32, u32)> {
    let x0 = x.saturating_sub(1);
    let y0 = y.saturating_sub(1);
    let x1 = x.saturating_add(1).min(image.width().saturating_sub(1));
    let y1 = y.saturating_add(1).min(image.height().saturating_sub(1));
    (y0..=y1)
        .flat_map(move |ny| (x0..=x1).map(move |nx| (nx, ny)))
        .filter(move |&(nx, ny)| nx != x || ny != y)
}

fn on_edge(image: &RgbaImage, x: u32, y: u32) -> bool {
    x == 0
        || y == 0
        || x.saturating_add(1) >= image.width()
        || y.saturating_add(1) >= image.height()
}

/// pixelmatch's anti-aliasing test: the pixel sits between a darkest and brightest neighbour
/// that both lie in flat areas of both images.
fn antialiased(image: &RgbaImage, x: u32, y: u32, other: &RgbaImage) -> bool {
    let centre = luma(pixel(image, x, y));
    let mut zeroes = u8::from(on_edge(image, x, y));
    let (mut min, mut max) = (0.0f64, 0.0f64);
    let (mut min_at, mut max_at) = ((x, y), (x, y));
    for (nx, ny) in neighbours(image, x, y) {
        let delta = centre - luma(pixel(image, nx, ny));
        if delta == 0.0 {
            zeroes = zeroes.saturating_add(1);
            if zeroes > 2 {
                return false;
            }
        } else if delta < min {
            min = delta;
            min_at = (nx, ny);
        } else if delta > max {
            max = delta;
            max_at = (nx, ny);
        }
    }
    if min == 0.0 || max == 0.0 {
        return false;
    }
    let flat =
        |(px, py): (u32, u32)| has_many_siblings(image, px, py) && has_many_siblings(other, px, py);
    flat(min_at) || flat(max_at)
}

/// Tolerates rasterization differences of at most one pixel, such as glyph stems that
/// FreeType and Skia place on neighbouring pixels: each image's color must occur within
/// the other image's 3x3 neighbourhood.
fn displaced(reference: &RgbaImage, safe: &RgbaImage, x: u32, y: u32) -> bool {
    let near = |source: &RgbaImage, target: &RgbaImage| {
        let wanted = pixel(source, x, y);
        neighbours(target, x, y)
            .any(|(nx, ny)| color_delta(wanted, pixel(target, nx, ny)) <= MAX_DELTA)
    };
    near(reference, safe) && near(safe, reference)
}

fn has_many_siblings(image: &RgbaImage, x: u32, y: u32) -> bool {
    let centre = pixel(image, x, y);
    let mut zeroes = u8::from(on_edge(image, x, y));
    for (nx, ny) in neighbours(image, x, y) {
        if pixel(image, nx, ny) == centre {
            zeroes = zeroes.saturating_add(1);
            if zeroes > 2 {
                return true;
            }
        }
    }
    false
}
