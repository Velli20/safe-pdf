//! Rectangle growth, alignment, and caret geometry for wrapped text.

use pdf_annotation_types::FreeTextAlignment;
use pdf_font::font::Font;
use pdf_graphics::rect::Rect;

use crate::{
    FreeTextEditError, FreeTextOverflow, FreeTextStyle, free_text_layout_wrapping::WrappedLine,
};

/// Borrows all state needed to calculate free-text geometry.
pub(super) struct LayoutGeometry<'a> {
    /// The normalized minimum annotation rectangle.
    rect: Rect,
    /// The validated layout style.
    style: &'a FreeTextStyle,
    /// The font used for line measurement.
    font: &'a Font,
    /// The wrapped lines in visual order.
    lines: &'a [WrappedLine],
}

impl<'a> LayoutGeometry<'a> {
    /// Creates a geometry calculator over a completed layout.
    pub(super) fn new(
        rect: Rect,
        style: &'a FreeTextStyle,
        font: &'a Font,
        lines: &'a [WrappedLine],
    ) -> Self {
        Self {
            rect,
            style,
            font,
            lines,
        }
    }

    /// Returns a rectangle enlarged according to the configured overflow policy.
    pub(super) fn grown_rect(&self) -> Result<Rect, FreeTextEditError> {
        let dimensions = self.grown_dimensions()?;
        Ok(Rect {
            left: self.rect.left,
            top: self.rect.top,
            right: self.rect.left + dimensions.width(),
            bottom: self.rect.top + dimensions.height(),
        })
    }

    /// Returns the caret rectangle for a character cursor.
    pub(super) fn caret_rect(
        &self,
        cursor: usize,
        character_count: usize,
    ) -> Result<Rect, FreeTextEditError> {
        if cursor > character_count {
            return Err(FreeTextEditError::InvalidCursor {
                cursor,
                character_count,
            });
        }

        let line_index = self.caret_line_index(cursor);
        let metrics = self.caret_metrics(line_index, cursor);
        let x = self.rect.left
            + self.line_x(self.rect.width(), metrics.line_width)
            + metrics.prefix_width;
        let y = self.rect.top + self.line_baseline(line_index);

        Ok(Rect {
            left: x,
            top: y,
            right: x + 1.0,
            bottom: y + self.style.font_size,
        })
    }

    /// Measures one wrapped line at the configured font size.
    pub(super) fn line_width(&self, line: &WrappedLine) -> f32 {
        self.font
            .encoded_text_width(line.bytes(), self.style.font_size)
    }

    /// Returns the horizontal text origin for a measured line.
    pub(super) fn line_x(&self, width: f32, line_width: f32) -> f32 {
        let content_width = width - self.style.insets.left - self.style.insets.right;
        let alignment_offset = match self.style.alignment {
            FreeTextAlignment::Left => 0.0,
            FreeTextAlignment::Center => (content_width - line_width) / 2.0,
            FreeTextAlignment::Right => content_width - line_width,
        };
        self.style.insets.left + alignment_offset.max(0.0)
    }

    /// Calculates finite dimensions for the overflow policy.
    fn grown_dimensions(&self) -> Result<Rect, FreeTextEditError> {
        let required_height = self.required_height()?;
        let height = self.grown_height(required_height)?;
        let required_width = self.required_width();
        let width = self.grown_width(required_width);
        let dimensions = Rect::new(width, height);
        if dimensions.is_valid() {
            Ok(dimensions)
        } else {
            Err(FreeTextEditError::invalid_input(
                "rectangle",
                "grown dimensions are not finite",
            ))
        }
    }

    /// Calculates the height required for all wrapped lines and vertical insets.
    fn required_height(&self) -> Result<f32, FreeTextEditError> {
        let line_count = u16::try_from(self.lines.len())
            .map_err(|_| FreeTextEditError::invalid_input("text", "too many wrapped lines"))?;
        Ok(self.style.insets.top
            + self.style.insets.bottom
            + self.style.line_height * f32::from(line_count))
    }

    /// Applies the vertical overflow policy to a required content height.
    fn grown_height(&self, required_height: f32) -> Result<f32, FreeTextEditError> {
        match self.style.overflow {
            FreeTextOverflow::ExpandHeight | FreeTextOverflow::ExpandRight => {
                Ok(self.rect.height().max(required_height))
            }
            FreeTextOverflow::Reject if required_height > self.rect.height() => {
                Err(FreeTextEditError::invalid_input(
                    "rectangle",
                    "text does not fit within the requested height",
                ))
            }
            FreeTextOverflow::Reject => Ok(self.rect.height()),
        }
    }

    /// Calculates the width required for the longest line and horizontal insets.
    fn required_width(&self) -> f32 {
        self.style.insets.left
            + self.style.insets.right
            + self
                .lines
                .iter()
                .map(|line| self.line_width(line))
                .fold(0.0_f32, f32::max)
    }

    /// Applies the horizontal overflow policy to a required content width.
    fn grown_width(&self, required_width: f32) -> f32 {
        match self.style.overflow {
            FreeTextOverflow::ExpandRight => self.rect.width().max(required_width),
            FreeTextOverflow::ExpandHeight | FreeTextOverflow::Reject => self.rect.width(),
        }
    }

    /// Selects the visual line associated with a source cursor.
    fn caret_line_index(&self, cursor: usize) -> usize {
        self.lines
            .iter()
            .position(|line| line.contains_cursor(cursor))
            .or_else(|| {
                self.lines
                    .iter()
                    .position(|line| cursor < line.source_start())
            })
            .unwrap_or_else(|| self.lines.len().saturating_sub(1))
    }

    /// Measures the selected line and the source prefix preceding the caret.
    fn caret_metrics(&self, line_index: usize, cursor: usize) -> CaretMetrics {
        let Some(line) = self.lines.get(line_index) else {
            return CaretMetrics::empty();
        };
        let prefix_length = cursor
            .saturating_sub(line.source_start())
            .min(line.bytes().len());
        let prefix = line.bytes().get(..prefix_length).unwrap_or_default();

        CaretMetrics {
            line_width: self.line_width(line),
            prefix_width: self.font.encoded_text_width(prefix, self.style.font_size),
        }
    }

    /// Returns the baseline height for a visual line index.
    fn line_baseline(&self, line_index: usize) -> f32 {
        self.rect.height()
            - self.style.insets.top
            - self.style.font_size
            - self.style.line_height * f32::from(u16::try_from(line_index).unwrap_or(u16::MAX))
    }
}

/// Width measurements needed to position a caret within one line.
struct CaretMetrics {
    /// The width of the complete visual line.
    line_width: f32,
    /// The width of the encoded prefix preceding the caret.
    prefix_width: f32,
}

impl CaretMetrics {
    /// Returns zero measurements for a defensive missing-line fallback.
    fn empty() -> Self {
        Self {
            line_width: 0.0,
            prefix_width: 0.0,
        }
    }
}
