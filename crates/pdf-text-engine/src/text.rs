//! PDF text inputs and backend-independent positioned glyph output.

use std::sync::Arc;

use pdf_font::pdf_font_handle::PdfFontHandle;
use pdf_font::{FontFace, GlyphId};
use pdf_graphics::{rect::Rect, transform::Transform};

pub use crate::text_style::TextStyle;
pub use pdf_content_stream_operators::PdfTextItem;

/// Borrowed PDF text input retaining the selected font resource and positioning operands.
pub struct PdfTextRun<'a> {
    /// Loaded PDF font resource.
    pub font: &'a PdfFontHandle,
    /// Text and positioning operands in source order.
    pub items: &'a [PdfTextItem],
    /// Current PDF text-state values.
    pub style: TextStyle,
}

/// Two-dimensional distance or offset in text-layout coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextVector {
    /// Horizontal component.
    pub x: f32,
    /// Vertical component.
    pub y: f32,
}

/// One glyph with an absolute layout position.
#[derive(Clone)]
pub struct PositionedGlyph {
    /// Face-specific glyph identifier.
    pub glyph_id: GlyphId,
    /// Transform from the face's glyph coordinates into layout coordinates.
    pub local_transform: Transform,
    /// Glyph bounds in layout coordinates.
    pub bounds: Rect,
    /// Unicode scalars represented by this glyph.
    pub unicode: pdf_cmap::UnicodeSequence,
}

/// Consecutive positioned glyphs sharing one face.
#[derive(Clone)]
pub struct GlyphRun {
    /// Shared face used by every glyph in the run.
    pub face: Arc<dyn FontFace>,
    /// Positioned glyphs in PDF paint order.
    pub glyphs: Vec<PositionedGlyph>,
}

/// Backend-independent result of PDF decoding, fallback selection, and positioning.
#[derive(Clone, Default)]
pub struct TextLayout {
    /// Face-homogeneous glyph runs in paint order.
    pub runs: Vec<GlyphRun>,
    /// Total pen advance from the layout origin.
    pub advance: TextVector,
}
