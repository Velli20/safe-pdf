use pdf_font::{char_vec::CharVec, font::Font};
use pdf_graphics::{rect::Rect, transform::Transform};

/// One selectable glyph span in rendered device coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct TextGlyph {
    /// Unicode scalar values represented by this glyph span.
    pub unicode: CharVec,
    /// Axis-aligned device-space bounds for hit testing and highlighting.
    pub bounds: Rect,
}

/// Minimal state captured before advancing a glyph.
///
/// This stays stack-only and is produced only while text recording is enabled.
#[derive(Clone, Copy)]
pub(crate) struct TextGlyphStart<'a> {
    pub(crate) transform: Transform,
    pub(crate) font_size: f32,
    pub(crate) font: Option<&'a Font>,
}

pub(crate) fn glyph_unicode(font: Option<&Font>, char_code: u16) -> CharVec {
    let mut unicode = font
        .map(|current_font| current_font.chars_to_unicode(char_code))
        .unwrap_or_default();
    if unicode.is_empty() {
        unicode.push(char::from_u32(u32::from(char_code)).unwrap_or('\u{fffd}'));
    }
    unicode
}

pub(crate) fn glyph_bounds(before: &Transform, font_size: f32, after: &Transform) -> Rect {
    let (start_x, start_y) = before.transform_point(0.0, 0.0);
    let (end_x, end_y) = after.transform_point(0.0, 0.0);
    let font_size = font_size.abs().max(1.0);
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

    #[test]
    fn glyph_bounds_cover_advanced_text_position() {
        let before = Transform::identity();
        let after = Transform::from_translate(24.0, 0.0);

        let bounds = glyph_bounds(&before, 12.0, &after);

        assert!(bounds.left <= 0.0);
        assert!(bounds.right >= 24.0);
        assert!(bounds.top < bounds.bottom);
    }

    #[test]
    fn glyph_bounds_apply_device_transform() {
        let before = Transform::from_translate(5.0, 7.0);
        let after = Transform::from_translate(15.0, 7.0);

        let bounds = glyph_bounds(&before, 10.0, &after);

        assert!(bounds.left <= 5.0);
        assert!(bounds.right >= 15.0);
        assert!(bounds.top <= 7.0);
    }

    #[test]
    fn glyph_bounds_respect_flipped_canvas_transform() {
        let before = Transform::from_row(1.0, 0.0, 0.0, -1.0, 30.0, 60.0);
        let after = Transform::from_row(1.0, 0.0, 0.0, -1.0, 40.0, 60.0);

        let bounds = glyph_bounds(&before, 20.0, &after);

        assert!(bounds.top < 60.0);
        assert!(bounds.bottom > 60.0);
        assert!(bounds.bottom - 60.0 < 60.0 - bounds.top);
    }

    #[test]
    fn glyph_bounds_normalize_rotated_advance() {
        let before = Transform::from_row(0.0, 1.0, -1.0, 0.0, 20.0, 30.0);
        let mut after = before;
        after.post_translate(10.0, 0.0);

        assert!(glyph_bounds(&before, 12.0, &after).is_valid());
    }
}
