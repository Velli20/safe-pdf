use pdf_font::font::Font;
use pdf_graphics::{rect::Rect, transform::Transform};

use crate::text_state::TextState;

/// One extracted text glyph or character span in rendered device coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct TextGlyph {
    /// Unicode text represented by this glyph span.
    pub text: String,
    /// Axis-aligned device-space bounds for hit testing and highlighting.
    pub bounds: Rect,
}

/// Receives text spans while a page content stream is rendered.
pub trait TextSink {
    /// Records one extracted text span.
    fn push_glyph(&mut self, glyph: TextGlyph);
}

/// Collects text spans emitted by [`PdfCanvas`](crate::pdf_canvas::PdfCanvas).
#[derive(Default)]
pub struct TextCollector {
    glyphs: Vec<TextGlyph>,
}

impl TextCollector {
    /// Creates an empty text collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the collected glyph spans.
    pub fn glyphs(&self) -> &[TextGlyph] {
        &self.glyphs
    }

    /// Consumes the collector and returns its glyph spans.
    pub fn into_glyphs(self) -> Vec<TextGlyph> {
        self.glyphs
    }
}

impl TextSink for TextCollector {
    fn push_glyph(&mut self, glyph: TextGlyph) {
        self.glyphs.push(glyph);
    }
}

pub(crate) fn glyph_text(font: Option<&Font>, char_code: u16) -> String {
    font.map(|current_font| current_font.chars_to_unicode(char_code))
        .filter(|chars| !chars.is_empty())
        .map(|chars| chars.iter().collect())
        .unwrap_or_else(|| {
            char::from_u32(u32::from(char_code))
                .unwrap_or('\u{fffd}')
                .to_string()
        })
}

pub(crate) fn glyph_bounds(
    text_state_before_advance: &TextState<'_>,
    ctm: &Transform,
    text_state_after_advance: &TextState<'_>,
) -> Rect {
    let mut before = text_state_before_advance.matrix;
    before.concat(ctm);

    let mut after = text_state_after_advance.matrix;
    after.concat(ctm);

    let (start_x, start_y) = before.transform_point(0.0, 0.0);
    let (end_x, end_y) = after.transform_point(0.0, 0.0);
    let font_size = text_state_before_advance.font_size.abs().max(1.0);
    let local_rect = Rect {
        left: 0.0,
        top: font_size * 0.8,
        right: font_size * 0.5,
        bottom: -font_size * 0.25,
    };
    let mut rect = before.map_rect(&local_rect);

    rect.left = rect.left.min(start_x).min(end_x);
    rect.right = rect.right.max(start_x).max(end_x);
    rect.top = rect.top.min(start_y).min(end_y);
    rect.bottom = rect.bottom.max(start_y).max(end_y);

    rect.normalized()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_state::TextState;

    #[test]
    fn glyph_bounds_cover_advanced_text_position() {
        let before = TextState {
            font_size: 12.0,
            ..Default::default()
        };
        let mut after = before.clone();
        after.matrix.post_translate(24.0, 0.0);

        let bounds = glyph_bounds(&before, &Transform::identity(), &after);

        assert!(bounds.left <= 0.0);
        assert!(bounds.right >= 24.0);
        assert!(bounds.top < bounds.bottom);
    }

    #[test]
    fn glyph_bounds_apply_ctm() {
        let before = TextState {
            font_size: 10.0,
            ..Default::default()
        };
        let mut after = before.clone();
        after.matrix.post_translate(10.0, 0.0);
        let ctm = Transform::from_translate(5.0, 7.0);

        let bounds = glyph_bounds(&before, &ctm, &after);

        assert!(bounds.left <= 5.0);
        assert!(bounds.right >= 15.0);
        assert!(bounds.top <= 7.0);
    }

    #[test]
    fn glyph_bounds_respect_flipped_canvas_ctm() {
        let mut before = TextState {
            font_size: 20.0,
            ..Default::default()
        };
        before.matrix.post_translate(30.0, 40.0);
        let mut after = before.clone();
        after.matrix.post_translate(10.0, 0.0);
        let ctm = Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, 100.0);

        let bounds = glyph_bounds(&before, &ctm, &after);

        let (_baseline_x, baseline_y) = before.matrix.transform_point(0.0, 0.0);
        let baseline_y = 100.0 - baseline_y;
        assert!(bounds.top < baseline_y);
        assert!(bounds.bottom > baseline_y);
        assert!(bounds.bottom - baseline_y < baseline_y - bounds.top);
    }
}
