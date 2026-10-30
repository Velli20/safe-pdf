//! Page text hit testing, selection geometry, and Unicode copying.

use pdf_graphics::{point::Point, rect::Rect};
use std::sync::OnceLock;

pub use crate::document_text_selection::{
    DocumentTextSelection, SelectionBatch, SelectionPoint, SelectionSpan, TextSelectionError,
    TextSelectionResult,
};
pub use pdf_canvas::text::TextGlyph;

/// Ordered text layout for one rendered page size.
#[derive(Debug, Clone, Default)]
pub struct PageTextLayout {
    glyphs: Vec<TextGlyph>,
    /// Built on the first hit test; most recorded layouts are never hit-tested.
    index: OnceLock<SpatialIndex>,
}

/// A spatial acceleration structure over valid glyph bounds.
#[derive(Debug, Clone, Default)]
struct SpatialIndex {
    nodes: Vec<IndexNode>,
}

/// An internal spatial-index node containing either a glyph or two children.
#[derive(Debug, Clone)]
struct IndexNode {
    bounds: Rect,
    children: Option<(usize, usize)>,
    glyph: usize,
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
        Self {
            glyphs,
            index: OnceLock::new(),
        }
    }

    /// Returns all glyph spans in content-stream order.
    pub fn glyphs(&self) -> &[TextGlyph] {
        &self.glyphs
    }

    /// Finds the containing or nearest glyph hit for a device-space point.
    pub fn hit_test(&self, point: Point) -> Option<TextHit> {
        self.index
            .get_or_init(|| SpatialIndex::new(&self.glyphs))
            .hit(point)
            .map(|index| TextHit { index })
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

impl SpatialIndex {
    /// Builds an index over the valid glyph rectangles.
    fn new(glyphs: &[TextGlyph]) -> Self {
        let mut entries: Vec<_> = glyphs
            .iter()
            .enumerate()
            .filter(|(_, g)| g.bounds.is_valid())
            .map(|(i, g)| (i, g.bounds))
            .collect();
        let mut result = Self { nodes: Vec::new() };
        if !entries.is_empty() {
            result.build(&mut entries);
        }
        result
    }

    /// Recursively builds a balanced binary index and returns the root node.
    fn build(&mut self, entries: &mut [(usize, Rect)]) -> usize {
        let bounds = entries.iter().fold(
            Rect {
                left: f32::INFINITY,
                top: f32::INFINITY,
                right: f32::NEG_INFINITY,
                bottom: f32::NEG_INFINITY,
            },
            |a, (_, b)| Rect {
                left: a.left.min(b.left),
                top: a.top.min(b.top),
                right: a.right.max(b.right),
                bottom: a.bottom.max(b.bottom),
            },
        );
        let index = self.nodes.len();
        self.nodes.push(IndexNode {
            bounds,
            children: None,
            glyph: entries.first().map(|v| v.0).unwrap_or_default(),
        });
        if entries.len() > 1 {
            entries.sort_by(|a, b| {
                if bounds.width() > bounds.height() {
                    a.1.left.total_cmp(&b.1.left)
                } else {
                    a.1.top.total_cmp(&b.1.top)
                }
            });
            let mid = entries.len() / 2;
            let (left, right) = entries.split_at_mut(mid);
            let children = (self.build(left), self.build(right));
            if let Some(node) = self.nodes.get_mut(index) {
                node.children = Some(children);
            }
        }
        index
    }

    /// Returns the containing or nearest indexed glyph for a device-space point.
    fn hit(&self, point: Point) -> Option<usize> {
        let mut stack = vec![0];
        let mut best: Option<(f32, usize)> = None;
        while let Some(index) = stack.pop() {
            let Some(node) = self.nodes.get(index) else {
                continue;
            };
            let distance = node.bounds.distance(point);
            if best.is_some_and(|(d, _)| distance > d) {
                continue;
            }
            if let Some((a, b)) = node.children {
                stack.push(b);
                stack.push(a);
            } else if best.is_none_or(|(d, i)| distance < d || (distance == d && node.glyph < i)) {
                best = Some((distance, node.glyph));
            }
        }
        best.map(|(_, i)| i)
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

        assert_eq!(
            layout.hit_test(Point::new(13.0, 5.0)).map(TextHit::index),
            Some(1)
        );
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
