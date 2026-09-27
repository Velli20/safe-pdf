//! Attaches what each renderer drew to the regions where the renders differ.

use crate::{
    model::{DrawRef, Region},
    regions::RawRegion,
};
use pdf_renderer::text_selection::PageTextLayout;

/// Draw calls listed per region.
const DRAWS_PER_REGION: usize = 8;
/// Characters of text kept per region and renderer.
const TEXT_LIMIT: usize = 240;

/// Builds attributed regions from raw regions.
///
/// `page_space` maps a pixel box to PDF user space; `reference_text` returns the text the
/// reference renderer extracts inside a user-space box (empty when it cannot).
pub fn attribute(
    raw: Vec<RawRegion>,
    page_space: &dyn Fn([u32; 4]) -> Option<[f32; 4]>,
    reference_text: &dyn Fn([f32; 4]) -> String,
    draws: &[DrawRef],
    layout: Option<&PageTextLayout>,
) -> Vec<Region> {
    raw.into_iter()
        .map(|region| {
            let bounds = page_space(region.pixels);
            Region {
                pixels: region.pixels,
                page_space: bounds,
                mismatched: region.mismatched,
                class: region.class,
                shift: region.shift,
                reference_text: bounds
                    .map(|bounds| truncate(reference_text(bounds).trim()))
                    .unwrap_or_default(),
                safe_text: layout
                    .map_or_else(String::new, |layout| safe_text(layout, region.pixels)),
                draws: overlapping(draws, region.pixels),
            }
        })
        .collect()
}

fn as_f32(pixels: [u32; 4]) -> [f32; 4] {
    pixels.map(|value| f32::from(u16::try_from(value).unwrap_or(u16::MAX)))
}

fn area([x0, y0, x1, y1]: [f32; 4]) -> f32 {
    (x1 - x0).max(0.0) * (y1 - y0).max(0.0)
}

fn intersection(a: [f32; 4], b: [f32; 4]) -> f32 {
    area([
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ])
}

/// Returns the draws overlapping the region, tightest fit first, so page-sized
/// backgrounds rank below the glyphs and images that actually cover the region.
fn overlapping(draws: &[DrawRef], pixels: [u32; 4]) -> Vec<DrawRef> {
    let region = as_f32(pixels);
    let region_area = area(region).max(1.0);
    let mut scored: Vec<(f32, &DrawRef)> = draws
        .iter()
        .filter_map(|draw| {
            let overlap = intersection(draw.bounds, region);
            (overlap > 0.0).then(|| {
                (
                    overlap * overlap / (area(draw.bounds).max(1.0) * region_area),
                    draw,
                )
            })
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored
        .into_iter()
        .take(DRAWS_PER_REGION)
        .map(|(_, draw)| draw.clone())
        .collect()
}

fn safe_text(layout: &PageTextLayout, pixels: [u32; 4]) -> String {
    let [x0, y0, x1, y1] = as_f32(pixels);
    let text: String = layout
        .glyphs()
        .iter()
        .filter(|glyph| {
            let bounds = glyph.bounds.normalized();
            let cx = (bounds.left + bounds.right) / 2.0;
            let cy = (bounds.top + bounds.bottom) / 2.0;
            (x0..=x1).contains(&cx) && (y0..=y1).contains(&cy)
        })
        .flat_map(|glyph| glyph.unicode.as_slice().iter().copied())
        .collect();
    truncate(text.trim())
}

fn truncate(text: &str) -> String {
    let mut result: String = text.chars().take(TEXT_LIMIT).collect();
    if text.chars().nth(TEXT_LIMIT).is_some() {
        result.push('…');
    }
    result
}
