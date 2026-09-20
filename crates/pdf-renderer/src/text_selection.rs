//! Page text hit testing, selection geometry, and Unicode copying.

use pdf_graphics::{point::Point, rect::Rect};

pub use crate::document_text_selection::{
    DocumentTextSelection, SelectionBatch, SelectionPoint, SelectionSpan, TextSelectionError,
    TextSelectionResult,
};
pub use pdf_canvas::text::TextGlyph;

/// Ordered text layout for one rendered page size.
#[derive(Debug, Clone, Default)]
pub struct PageTextLayout {
    glyphs: Vec<TextGlyph>,
}

/// A hit-testable text position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextHit {
    index: usize,
}

/// A selected inclusive glyph range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    start: usize,
    end: usize,
}

impl PageTextLayout {
    /// Creates a page text layout from ordered glyph spans.
    pub fn new(glyphs: Vec<TextGlyph>) -> Self {
        Self { glyphs }
    }

    /// Returns all glyph spans in content-stream order.
    pub fn glyphs(&self) -> &[TextGlyph] {
        &self.glyphs
    }

    /// Finds the nearest glyph hit for a device-space point.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<TextHit> {
        let direct = self.glyphs.iter().enumerate().find_map(|(index, glyph)| {
            glyph
                .bounds
                .contains_point(x, y)
                .then_some(TextHit { index })
        });
        if direct.is_some() {
            return direct;
        }

        self.glyphs
            .iter()
            .enumerate()
            .filter_map(|(index, glyph)| {
                let distance = glyph.bounds.normalized().distance(Point::new(x, y));
                distance.is_finite().then_some((index, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| TextHit { index })
    }

    /// Builds a normalized inclusive selection between two hits.
    pub fn selection_between(&self, anchor: TextHit, focus: TextHit) -> Option<TextSelection> {
        self.selection_from_indices(anchor.index, focus.index)
    }

    /// Builds a normalized inclusive selection from glyph indices.
    pub fn selection_from_indices(&self, anchor: usize, focus: usize) -> Option<TextSelection> {
        let last_index = self.glyphs.len().checked_sub(1)?;
        let start = anchor.min(focus).min(last_index);
        let end = anchor.max(focus).min(last_index);
        Some(TextSelection { start, end })
    }

    /// Returns highlight rectangles for a selection.
    pub fn selection_rects(&self, selection: TextSelection) -> Vec<Rect> {
        self.selection_bounds(selection)
            .map(|(_, bounds)| bounds)
            .collect()
    }

    /// Returns copied text for a selection.
    pub fn selected_text(&self, selection: TextSelection) -> String {
        let mut result = String::new();
        let mut previous: Option<&TextGlyph> = None;

        for (_, glyph) in self.selected_glyphs(selection) {
            if let Some(prev) = previous
                && is_new_line(prev, glyph)
                && !result.ends_with('\n')
            {
                result.push('\n');
            }
            result.extend(glyph.unicode.as_slice());
            previous = Some(glyph);
        }

        result
    }

    /// Iterates valid highlight bounds with their original glyph indices.
    pub(super) fn selection_bounds(
        &self,
        selection: TextSelection,
    ) -> impl Iterator<Item = (usize, Rect)> {
        self.selected_glyphs(selection)
            .filter_map(|(index, glyph)| glyph.bounds.is_valid().then_some((index, glyph.bounds)))
    }

    /// Iterates the clamped inclusive selection, preserving original glyph indices.
    fn selected_glyphs(
        &self,
        selection: TextSelection,
    ) -> impl Iterator<Item = (usize, &TextGlyph)> {
        let start = selection.start.min(self.glyphs.len());
        let end_exclusive = selection
            .end
            .checked_add(1)
            .map(|end| end.min(self.glyphs.len()))
            .unwrap_or(self.glyphs.len());
        self.glyphs
            .iter()
            .enumerate()
            .skip(start)
            .take(end_exclusive.saturating_sub(start))
    }
}

impl TextHit {
    /// Returns the underlying glyph index in content-stream order.
    pub fn index(self) -> usize {
        self.index
    }
}

impl TextSelection {
    /// Returns the inclusive glyph-index range.
    pub fn range(self) -> (usize, usize) {
        (self.start, self.end)
    }
}

/// Detects a line break from the vertical gap relative to the glyph heights.
fn is_new_line(previous: &TextGlyph, current: &TextGlyph) -> bool {
    let previous_height = previous.bounds.height().abs().max(1.0);
    let current_height = current.bounds.height().abs().max(1.0);
    let threshold = previous_height.max(current_height) * 0.6;
    current.bounds.top > previous.bounds.bottom + threshold
}

#[cfg(test)]
/// Tests page hit testing, range normalization, and Unicode copying.
mod tests {
    use super::*;
    use std::sync::Arc;

    use pdf_cmap::UnicodeSequence;

    /// Creates a glyph with the supplied Unicode text and bounds.
    fn glyph(text: &str, left: f32, top: f32, right: f32, bottom: f32) -> TextGlyph {
        TextGlyph {
            unicode: UnicodeSequence::from_shared(Arc::from(text.chars().collect::<Vec<_>>())),
            bounds: Rect {
                left,
                top,
                right,
                bottom,
            },
        }
    }

    #[test]
    /// Checks that a containing glyph takes precedence over nearby glyphs.
    fn hit_test_returns_containing_glyph() {
        let layout = PageTextLayout::new(vec![
            glyph("a", 0.0, 0.0, 10.0, 10.0),
            glyph("b", 12.0, 0.0, 20.0, 10.0),
        ]);

        assert_eq!(layout.hit_test(13.0, 5.0).map(TextHit::index), Some(1));
    }

    #[test]
    /// Checks that a reverse drag produces an ordered inclusive selection.
    fn selection_between_normalizes_reverse_drag() {
        let layout = PageTextLayout::new(vec![
            glyph("a", 0.0, 0.0, 10.0, 10.0),
            glyph("b", 12.0, 0.0, 20.0, 10.0),
        ]);

        let selection = layout
            .selection_between(TextHit { index: 1 }, TextHit { index: 0 })
            .expect("selection should exist");

        assert_eq!(selection.range(), (0, 1));
        assert_eq!(layout.selected_text(selection), "ab");
    }

    #[test]
    /// Checks that oversized indices are clamped to the layout.
    fn selection_from_indices_clamps_to_layout() {
        let layout = PageTextLayout::new(vec![
            glyph("a", 0.0, 0.0, 10.0, 10.0),
            glyph("b", 12.0, 0.0, 20.0, 10.0),
        ]);

        let selection = layout
            .selection_from_indices(usize::MAX, 0)
            .expect("selection should exist");

        assert_eq!(selection.range(), (0, 1));
    }

    #[test]
    /// Checks that an empty layout has no selectable range.
    fn selection_from_indices_returns_none_for_empty_layout() {
        let layout = PageTextLayout::new(Vec::new());

        assert_eq!(layout.selection_from_indices(0, 0), None);
    }

    #[test]
    /// Checks that copying across a vertical gap inserts a newline.
    fn selected_text_inserts_newline_between_lines() {
        let layout = PageTextLayout::new(vec![
            glyph("a", 0.0, 0.0, 10.0, 10.0),
            glyph("b", 0.0, 24.0, 10.0, 34.0),
        ]);
        let selection = layout
            .selection_between(TextHit { index: 0 }, TextHit { index: 1 })
            .expect("selection should exist");

        assert_eq!(layout.selected_text(selection), "a\nb");
    }

    #[test]
    /// Checks that copying retains every Unicode scalar in a glyph.
    fn selected_text_preserves_multi_scalar_glyphs() {
        let layout = PageTextLayout::new(vec![glyph("fi", 0.0, 0.0, 10.0, 10.0)]);
        let selection = layout
            .selection_from_indices(0, 0)
            .expect("selection should exist");

        assert_eq!(layout.selected_text(selection), "fi");
    }
}
