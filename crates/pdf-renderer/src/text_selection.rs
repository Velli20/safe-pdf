use pdf_graphics::rect::Rect;

/// Ordered text layout for one rendered page size.
#[derive(Debug, Clone, Default)]
pub struct PageTextLayout {
    glyphs: Vec<TextGlyph>,
}

/// One selectable text span in device coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct TextGlyph {
    /// Unicode text represented by this span.
    pub text: String,
    /// Axis-aligned device-space bounds.
    pub bounds: Rect,
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
            contains_point(&glyph.bounds, x, y).then_some(TextHit { index })
        });
        if direct.is_some() {
            return direct;
        }

        self.glyphs
            .iter()
            .enumerate()
            .filter_map(|(index, glyph)| {
                let distance = distance_to_rect(&glyph.bounds, x, y);
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
        self.selected_glyphs(selection)
            .filter_map(|glyph| glyph.bounds.is_valid().then_some(glyph.bounds))
            .collect()
    }

    /// Returns copied text for a selection.
    pub fn selected_text(&self, selection: TextSelection) -> String {
        let mut result = String::new();
        let mut previous: Option<&TextGlyph> = None;

        for glyph in self.selected_glyphs(selection) {
            if let Some(prev) = previous
                && is_new_line(prev, glyph)
                && !result.ends_with('\n')
            {
                result.push('\n');
            }
            result.push_str(&glyph.text);
            previous = Some(glyph);
        }

        result
    }

    fn selected_glyphs(&self, selection: TextSelection) -> impl Iterator<Item = &TextGlyph> {
        let start = selection.start.min(self.glyphs.len());
        let end_exclusive = selection
            .end
            .checked_add(1)
            .map(|end| end.min(self.glyphs.len()))
            .unwrap_or(self.glyphs.len());
        self.glyphs
            .iter()
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

impl From<pdf_canvas::text::TextGlyph> for TextGlyph {
    fn from(value: pdf_canvas::text::TextGlyph) -> Self {
        Self {
            text: value.text,
            bounds: value.bounds,
        }
    }
}

fn contains_point(rect: &Rect, x: f32, y: f32) -> bool {
    let rect = rect.normalized();
    x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

fn distance_to_rect(rect: &Rect, x: f32, y: f32) -> f32 {
    let rect = rect.normalized();
    let dx = if x < rect.left {
        rect.left - x
    } else if x > rect.right {
        x - rect.right
    } else {
        0.0
    };
    let dy = if y < rect.top {
        rect.top - y
    } else if y > rect.bottom {
        y - rect.bottom
    } else {
        0.0
    };
    dx.hypot(dy)
}

fn is_new_line(previous: &TextGlyph, current: &TextGlyph) -> bool {
    let previous_height = previous.bounds.height().abs().max(1.0);
    let current_height = current.bounds.height().abs().max(1.0);
    let threshold = previous_height.max(current_height) * 0.6;
    current.bounds.top > previous.bounds.bottom + threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(text: &str, left: f32, top: f32, right: f32, bottom: f32) -> TextGlyph {
        TextGlyph {
            text: text.to_string(),
            bounds: Rect {
                left,
                top,
                right,
                bottom,
            },
        }
    }

    #[test]
    fn hit_test_returns_containing_glyph() {
        let layout = PageTextLayout::new(vec![
            glyph("a", 0.0, 0.0, 10.0, 10.0),
            glyph("b", 12.0, 0.0, 20.0, 10.0),
        ]);

        assert_eq!(layout.hit_test(13.0, 5.0).map(TextHit::index), Some(1));
    }

    #[test]
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
    fn selection_from_indices_returns_none_for_empty_layout() {
        let layout = PageTextLayout::new(Vec::new());

        assert_eq!(layout.selection_from_indices(0, 0), None);
    }

    #[test]
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
}
