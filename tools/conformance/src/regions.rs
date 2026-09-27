//! Groups differing pixels into regions and classifies how each region deviates.

use crate::{
    compare::{PixelDiff, luma, pixel, pixel_count, to_f64},
    model::RegionClass,
};
use image::RgbaImage;
use std::collections::VecDeque;

/// Side of the square cells differing pixels are binned into before grouping.
const CELL: u32 = 8;
/// Cells within this distance join the same region, bridging gaps between glyphs.
const REACH: u32 = 2;
/// Regions with fewer differing pixels are noise.
const MIN_PIXELS: u64 = 12;
/// Largest offset tried when checking whether a region is shifted content.
const MAX_SHIFT: i32 = 4;

/// A region before attribution.
pub struct RawRegion {
    /// Pixel bounds `[x0, y0, x1, y1]`, exclusive of `x1`/`y1`.
    pub pixels: [u32; 4],
    /// Number of differing pixels.
    pub mismatched: u64,
    /// Dominant deviation.
    pub class: RegionClass,
    /// Best aligning offset, for [`RegionClass::Offset`].
    pub shift: Option<[i32; 2]>,
}

#[derive(Clone, Copy, Default)]
struct Cell {
    missing: u32,
    extra: u32,
    color: u32,
    shape: u32,
}

impl Cell {
    fn total(self) -> u32 {
        self.missing
            .saturating_add(self.extra)
            .saturating_add(self.color)
            .saturating_add(self.shape)
    }
}

/// Finds up to `limit` regions, most differing pixels first.
pub fn find(
    mask: &[PixelDiff],
    reference: &RgbaImage,
    safe: &RgbaImage,
    limit: usize,
) -> Vec<RawRegion> {
    let (width, height) = reference.dimensions();
    let columns = width.div_ceil(CELL);
    let rows = height.div_ceil(CELL);
    let mut cells = vec![Cell::default(); pixel_count(columns, rows)];
    let index = |cx: u32, cy: u32| {
        u64::from(cy)
            .checked_mul(u64::from(columns))
            .and_then(|row| row.checked_add(u64::from(cx)))
            .and_then(|position| usize::try_from(position).ok())
            .unwrap_or(usize::MAX)
    };
    let row_length = usize::try_from(width).unwrap_or(usize::MAX).max(1);
    let pixels = mask.chunks(row_length).zip(0u32..).flat_map(|(row, y)| {
        row.iter()
            .zip(0u32..)
            .map(move |(outcome, x)| (x, y, outcome))
    });
    for (x, y, outcome) in pixels {
        if let Some(cell) = cells.get_mut(index(x / CELL, y / CELL)) {
            match outcome {
                PixelDiff::Same => {}
                PixelDiff::Missing => cell.missing = cell.missing.saturating_add(1),
                PixelDiff::Extra => cell.extra = cell.extra.saturating_add(1),
                PixelDiff::Color => cell.color = cell.color.saturating_add(1),
                PixelDiff::Shape => cell.shape = cell.shape.saturating_add(1),
            }
        }
    }
    let mut visited = vec![false; cells.len()];
    let mut regions = Vec::new();
    for cy in 0..rows {
        for cx in 0..columns {
            let start = index(cx, cy);
            if visited.get(start).copied().unwrap_or(true)
                || cells.get(start).is_none_or(|cell| cell.total() == 0)
            {
                continue;
            }
            if let Some(flag) = visited.get_mut(start) {
                *flag = true;
            }
            let mut sum = Cell::default();
            let mut bounds = [cx, cy, cx, cy];
            let mut queue = VecDeque::from([(cx, cy)]);
            while let Some((x, y)) = queue.pop_front() {
                let cell = cells.get(index(x, y)).copied().unwrap_or_default();
                sum.missing = sum.missing.saturating_add(cell.missing);
                sum.extra = sum.extra.saturating_add(cell.extra);
                sum.color = sum.color.saturating_add(cell.color);
                sum.shape = sum.shape.saturating_add(cell.shape);
                bounds = [
                    bounds[0].min(x),
                    bounds[1].min(y),
                    bounds[2].max(x),
                    bounds[3].max(y),
                ];
                let nx0 = x.saturating_sub(REACH);
                let ny0 = y.saturating_sub(REACH);
                let nx1 = x.saturating_add(REACH).min(columns.saturating_sub(1));
                let ny1 = y.saturating_add(REACH).min(rows.saturating_sub(1));
                for ny in ny0..=ny1 {
                    for nx in nx0..=nx1 {
                        let next = index(nx, ny);
                        if visited.get(next).copied().unwrap_or(true)
                            || cells.get(next).is_none_or(|cell| cell.total() == 0)
                        {
                            continue;
                        }
                        if let Some(flag) = visited.get_mut(next) {
                            *flag = true;
                        }
                        queue.push_back((nx, ny));
                    }
                }
            }
            let mismatched = u64::from(sum.total());
            if mismatched < MIN_PIXELS {
                continue;
            }
            let pixels = [
                bounds[0].saturating_mul(CELL),
                bounds[1].saturating_mul(CELL),
                bounds[2].saturating_add(1).saturating_mul(CELL).min(width),
                bounds[3].saturating_add(1).saturating_mul(CELL).min(height),
            ];
            regions.push((pixels, mismatched, sum));
        }
    }
    regions.sort_by_key(|region| std::cmp::Reverse(region.1));
    regions
        .into_iter()
        .take(limit)
        .map(|(pixels, mismatched, sum)| {
            let shift = best_shift(reference, safe, pixels);
            let dominant =
                |part: u32| u64::from(part).saturating_mul(3) > mismatched.saturating_mul(2);
            let class = if shift.is_some() {
                RegionClass::Offset
            } else if dominant(sum.missing) {
                RegionClass::MissingInk
            } else if dominant(sum.extra) {
                RegionClass::ExtraInk
            } else if u64::from(sum.color).saturating_mul(2) > mismatched {
                RegionClass::ColorShift
            } else {
                RegionClass::Reshaped
            };
            RawRegion {
                pixels,
                mismatched,
                class,
                shift,
            }
        })
        .collect()
}

/// Returns the offset of Safe-PDF content that removes most of the region's differences.
fn best_shift(
    reference: &RgbaImage,
    safe: &RgbaImage,
    [x0, y0, x1, y1]: [u32; 4],
) -> Option<[i32; 2]> {
    let area = u64::from(x1.saturating_sub(x0)).saturating_mul(u64::from(y1.saturating_sub(y0)));
    // Sample large regions so the search stays fast in unoptimized builds.
    let step = if area > 250_000 { 3 } else { 1 };
    let differing = |dx: i32, dy: i32| {
        let mut count = 0u64;
        for y in (y0..y1).step_by(step) {
            for x in (x0..x1).step_by(step) {
                let (Some(sx), Some(sy)) = (x.checked_add_signed(dx), y.checked_add_signed(dy))
                else {
                    count = count.saturating_add(1);
                    continue;
                };
                let a = luma(pixel(reference, x, y));
                let b = luma(pixel(safe, sx, sy));
                if (a - b).abs() > 48.0 {
                    count = count.saturating_add(1);
                }
            }
        }
        count
    };
    let baseline = differing(0, 0);
    if baseline == 0 {
        return None;
    }
    let mut best = (0, 0, baseline);
    for dy in -MAX_SHIFT..=MAX_SHIFT {
        for dx in -MAX_SHIFT..=MAX_SHIFT {
            if dx == 0 && dy == 0 {
                continue;
            }
            let count = differing(dx, dy);
            if count < best.2 {
                best = (dx, dy, count);
            }
        }
    }
    (to_f64(best.2) < to_f64(baseline) * 0.35).then_some([best.0, best.1])
}
