//! Annotation rectangles on the raster, labelled by annotation kind, so differences drawn by
//! the annotation layer are told apart from differences in page content.

use crate::compare::{PixelDiff, pixel_count, to_f64};
use num_traits::ToPrimitive;
use pdf_annotation_core::{NativeAnnotation, SourceAnnotation, WidgetAnnotation, fields::TextMode};
use pdf_document::page::PdfPage;
use pdf_graphics::size::Size;
use std::collections::BTreeMap;

/// Pixels an annotation box is grown by on each side, for anti-aliased edges.
const GROWTH: f32 = 2.0;

/// One annotation's device-pixel box and its kind.
pub struct AnnotationArea {
    /// Kind label such as `Widget (checkbox)`, `Stamp` or `FreeText`.
    pub kind: String,
    /// Pixel bounds `[x0, y0, x1, y1]`, exclusive of `x1`/`y1`, clamped to the image.
    pub pixels: [u32; 4],
    /// True for subtypes `pdfium_test` draws no appearance for (links and popups).
    pub unpainted_in_goldens: bool,
}

impl AnnotationArea {
    /// Returns the areas of the page's annotations on a `width` x `height` raster, in
    /// `/Annots` order. Annotations without a rectangle are left out.
    pub fn of_page(page: &PdfPage, width: u32, height: u32) -> Vec<Self> {
        let Some(device) = width.to_f32().zip(height.to_f32()) else {
            return Vec::new();
        };
        let Ok(viewport) = page.viewport(None, Size::new(device.0, device.1)) else {
            return Vec::new();
        };
        page.annotations
            .iter()
            .flatten()
            .filter_map(|annotation| {
                let mapped = viewport.map_rect(&annotation.rect?).ok()?;
                let clamp = |value: f32, limit: u32| value.max(0.0).to_u32().map(|v| v.min(limit));
                Some(Self {
                    kind: kind(annotation),
                    pixels: [
                        clamp((mapped.left - GROWTH).floor(), width)?,
                        clamp((mapped.top - GROWTH).floor(), height)?,
                        clamp((mapped.right + GROWTH).ceil(), width)?,
                        clamp((mapped.bottom + GROWTH).ceil(), height)?,
                    ],
                    unpainted_in_goldens: matches!(
                        annotation.kind,
                        NativeAnnotation::Link(_) | NativeAnnotation::Popup(_)
                    ),
                })
            })
            .collect()
    }
}

/// Returns the kind label of an annotation: the subtype name, with the field kind for widgets.
fn kind(annotation: &SourceAnnotation) -> String {
    match &annotation.kind {
        NativeAnnotation::Widget(widget) => match widget_kind(widget) {
            Some(field) => format!("Widget ({field})"),
            None => "Widget".to_owned(),
        },
        _ => String::from_utf8_lossy(&annotation.subtype).into_owned(),
    }
}

/// Returns the form field kind a widget presents, from its inherited `/FT` and `/Ff`.
fn widget_kind(widget: &WidgetAnnotation) -> Option<&'static str> {
    Some(match widget.field_type.as_deref()? {
        b"Btn" if widget.is_push_button() => "push button",
        b"Btn" if widget.is_radio_button() => "radio",
        b"Btn" => "checkbox",
        b"Tx" if widget.text_mode() == TextMode::Multiline => "multiline text",
        b"Tx" => "text",
        b"Ch" if widget.is_combo_box() => "combo box",
        b"Ch" => "list box",
        b"Sig" => "signature",
        _ => return None,
    })
}

/// Which annotation, if any, owns each pixel of the raster.
pub struct AnnotationMap {
    areas: Vec<AnnotationArea>,
    /// Row-major index into `areas` of the topmost annotation covering each pixel.
    owners: Vec<Option<usize>>,
    width: u32,
}

impl AnnotationMap {
    /// Maps the page's annotations onto a `width` x `height` raster. Where annotations
    /// overlap, the later one in `/Annots` owns the pixel, since it is painted on top.
    pub fn of_page(page: &PdfPage, width: u32, height: u32) -> Self {
        let areas = AnnotationArea::of_page(page, width, height);
        let mut owners = vec![None; pixel_count(width, height)];
        let row_length = usize::try_from(width).unwrap_or(usize::MAX).max(1);
        for (index, area) in areas.iter().enumerate() {
            let [x0, y0, x1, y1] = area.pixels;
            let columns = usize::try_from(x0).unwrap_or(usize::MAX)
                ..usize::try_from(x1).unwrap_or(usize::MAX);
            for row in owners
                .chunks_mut(row_length)
                .skip(usize::try_from(y0).unwrap_or(usize::MAX))
                .take(usize::try_from(y1.saturating_sub(y0)).unwrap_or(0))
            {
                if let Some(cells) = row.get_mut(columns.clone()) {
                    cells.fill(Some(index));
                }
            }
        }
        Self {
            areas,
            owners,
            width,
        }
    }

    /// Returns true when no annotation covers the raster.
    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }

    /// Returns `mask` with the pixels inside annotations set to [`PixelDiff::Same`].
    pub fn content_mask(&self, mask: &[PixelDiff]) -> Vec<PixelDiff> {
        self.filtered(mask, |owner| owner.is_none())
    }

    /// Returns `mask` with the pixels outside annotations set to [`PixelDiff::Same`].
    pub fn annotation_mask(&self, mask: &[PixelDiff]) -> Vec<PixelDiff> {
        self.filtered(mask, |owner| owner.is_some())
    }

    fn filtered(&self, mask: &[PixelDiff], keep: impl Fn(Option<usize>) -> bool) -> Vec<PixelDiff> {
        mask.iter()
            .zip(&self.owners)
            .map(|(&outcome, &owner)| {
                if keep(owner) {
                    outcome
                } else {
                    PixelDiff::Same
                }
            })
            .collect()
    }

    /// Returns the kind of the annotation owning most differing pixels inside `pixels`.
    pub fn label(&self, mask: &[PixelDiff], [x0, y0, x1, y1]: [u32; 4]) -> Option<String> {
        let mut counts: BTreeMap<usize, u64> = BTreeMap::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let index = u64::from(y)
                    .checked_mul(u64::from(self.width))
                    .and_then(|row| row.checked_add(u64::from(x)))
                    .and_then(|index| usize::try_from(index).ok());
                let Some(index) = index else { continue };
                if mask
                    .get(index)
                    .is_some_and(|outcome| *outcome != PixelDiff::Same)
                    && let Some(Some(owner)) = self.owners.get(index)
                {
                    let count = counts.entry(*owner).or_default();
                    *count = count.saturating_add(1);
                }
            }
        }
        let (owner, _) = counts.into_iter().max_by_key(|(_, count)| *count)?;
        self.areas.get(owner).map(|area| area.kind.clone())
    }

    /// Returns the fraction of raster pixels that differ inside annotations.
    pub fn mismatch(&self, mask: &[PixelDiff]) -> f64 {
        let differing = mask
            .iter()
            .zip(&self.owners)
            .filter(|(outcome, owner)| **outcome != PixelDiff::Same && owner.is_some())
            .count();
        to_f64(u64::try_from(differing).unwrap_or(u64::MAX))
            / to_f64(u64::try_from(mask.len()).unwrap_or(u64::MAX)).max(1.0)
    }
}
