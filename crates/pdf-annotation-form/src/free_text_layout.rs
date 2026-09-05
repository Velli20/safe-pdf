//! Validated text wrapping and geometry for editable free-text annotations.

use pdf_font::PdfFontSpec;
use pdf_graphics::rect::Rect;

use crate::{
    FreeTextStyle,
    free_text_layout_geometry::LayoutGeometry,
    free_text_layout_validation::ValidatedFreeText,
    free_text_layout_wrapping::{TextWrapper, WrappedLine},
};

use super::{FreeText, FreeTextEditError};

/// A validated and wrapped representation of an editable free-text annotation.
pub(crate) struct FreeTextLayout<'a> {
    /// The normalized annotation rectangle.
    rect: Rect,
    /// The validated appearance style.
    style: &'a FreeTextStyle,
    /// The font used to measure and render encoded text.
    font: PdfFontSpec,
    /// The encoded lines produced by the wrapping policy.
    lines: Vec<WrappedLine>,
    /// The number of character cursor positions in the source text.
    character_count: usize,
}

impl<'a> FreeTextLayout<'a> {
    /// Validates, encodes, and wraps an editable free-text annotation.
    pub(crate) fn new(free_text: &'a FreeText) -> Result<Self, FreeTextEditError> {
        let validated = ValidatedFreeText::try_from(free_text)?;
        let lines = TextWrapper::new(
            validated.font(),
            validated.style().font_size,
            validated.maximum_line_width(),
        )
        .wrap(validated.encoded_text());
        let rect = validated.rect();
        let style = validated.style();
        let character_count = validated.character_count();
        let font = validated.into_font();

        Ok(Self {
            rect,
            style,
            font,
            lines,
            character_count,
        })
    }

    /// Returns a rectangle enlarged according to the configured overflow policy.
    pub(crate) fn grown_rect(&self) -> Result<Rect, FreeTextEditError> {
        self.geometry().grown_rect()
    }

    /// Returns the caret rectangle for a validated character cursor.
    pub(super) fn caret_rect(&self, cursor: usize) -> Result<Rect, FreeTextEditError> {
        self.geometry().caret_rect(cursor, self.character_count)
    }

    /// Returns the wrapped lines in visual order.
    pub(crate) fn lines(&self) -> &[WrappedLine] {
        &self.lines
    }

    /// Consumes the layout and returns its rendering font.
    pub(crate) fn into_font(self) -> PdfFontSpec {
        self.font
    }

    /// Measures one wrapped line at the configured font size.
    pub(crate) fn line_width(&self, line: &WrappedLine) -> f32 {
        self.geometry().line_width(line)
    }

    /// Returns the horizontal text origin for a measured line.
    pub(crate) fn line_x(&self, width: f32, line_width: f32) -> f32 {
        self.geometry().line_x(width, line_width)
    }

    /// Borrows the inputs needed for geometric layout calculations.
    fn geometry(&self) -> LayoutGeometry<'_> {
        LayoutGeometry::new(self.rect, self.style, &self.font, &self.lines)
    }
}
