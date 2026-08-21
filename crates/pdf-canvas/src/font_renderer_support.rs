use crate::{
    canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas,
    text_state::TextState,
};
use pdf_font::{font::Font, glyph_name_to_unicode::glyph_name_to_unicode};
use pdf_graphics::transform::Transform;
use read_fonts::TableProvider;
use skrifa::{FontRef, GlyphId, charmap::Charmap};

/// Normalizes a parsed `units_per_em` value into the range accepted by text rendering.
///
/// # Parameters
///
/// - `units_per_em`: The parsed `units_per_em` value, if present.
///
/// # Returns
///
/// The normalized `units_per_em`, or `TextState::DEFAULT_UNITS_PER_EM` when the input is
/// missing or out of range.
pub(crate) fn normalized_units_per_em(units_per_em: Option<u16>) -> u16 {
    units_per_em
        .filter(|&upe| (TextState::MIN_UNITS_PER_EM..=TextState::MAX_UNITS_PER_EM).contains(&upe))
        .unwrap_or(TextState::DEFAULT_UNITS_PER_EM)
}

/// Builds the glyph base transform for the current text state.
///
/// # Parameters
///
/// - `canvas`: The canvas whose current text state provides glyph placement settings.
/// - `units_per_em`: The font design units per em.
///
/// # Returns
///
/// The glyph base transform derived from the current text state and font scaling.
pub(crate) fn glyph_base_transform<B: CanvasBackend>(
    canvas: &PdfCanvas<'_, B>,
    units_per_em: u16,
) -> Result<Transform, PdfCanvasError> {
    let units_per_em_inverse = 1.0 / f32::from(units_per_em);
    Ok(canvas
        .current_state()?
        .text_state
        .glyph_base_transform(units_per_em_inverse))
}

/// Resolves a glyph identifier for a simple font using the configured fallback order.
///
/// # Parameters
///
/// - `font`: The current PDF font, if one is attached to the text state.
/// - `code`: The source character code.
/// - `glyph_name`: The glyph name associated with `code`, if available.
/// - `charmap`: The font charmap used for Unicode-to-glyph lookup.
/// - `font_ref`: The parsed font reference used for cmap fallback lookup.
/// - `is_symbolic`: Whether symbolic codepoint fallback is allowed.
///
/// # Returns
///
/// The resolved glyph identifier, or `GlyphId::NOTDEF` when no mapping succeeds.
pub(crate) fn resolve_simple_font_gid(
    font: Option<&Font>,
    code: u16,
    glyph_name: Option<&[u8]>,
    charmap: &Charmap<'_>,
    font_ref: &FontRef<'_>,
    is_symbolic: bool,
) -> GlyphId {
    pdf_unicode_gid(font, code, charmap)
        .or_else(|| glyph_name_gid(glyph_name, charmap))
        .or_else(|| direct_cmap_gid(font_ref, code))
        .or_else(|| symbolic_gid(code, charmap, is_symbolic))
        .unwrap_or(GlyphId::NOTDEF)
}

fn pdf_unicode_gid(font: Option<&Font>, code: u16, charmap: &Charmap<'_>) -> Option<GlyphId> {
    font.and_then(|current_font| current_font.char_to_unicode(code))
        .and_then(|unicode_char| charmap.map(unicode_char))
}

fn glyph_name_gid(glyph_name: Option<&[u8]>, charmap: &Charmap<'_>) -> Option<GlyphId> {
    glyph_name
        .and_then(glyph_name_to_unicode)
        .and_then(|unicode_char| charmap.map(unicode_char))
}

fn direct_cmap_gid(font_ref: &FontRef<'_>, code: u16) -> Option<GlyphId> {
    font_ref
        .cmap()
        .ok()
        .and_then(|cmap| cmap.best_subtable())
        .and_then(|(_, _, subtable)| subtable.map_codepoint(code))
}

fn symbolic_gid(code: u16, charmap: &Charmap<'_>, is_symbolic: bool) -> Option<GlyphId> {
    is_symbolic
        .then(|| char::from_u32(u32::from(code)))
        .flatten()
        .and_then(|unicode_char| charmap.map(unicode_char))
}
