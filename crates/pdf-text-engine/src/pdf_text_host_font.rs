//! The host font program of a PDF font: the program its glyphs are drawn from, with each
//! character mapped onto the glyph layout selects for it.

use std::collections::BTreeMap;

use pdf_cmap::UnicodeSequence;
use pdf_font::{
    GlyphId, PdfFontSpec,
    host_font_program::{HostFontProgram, HostGlyph},
};

use crate::{
    error::TextError, pdf_text_decoder::decode_one_byte_mapping, pdf_text_layout::select_glyph,
    system::FontSystem,
};

impl FontSystem {
    /// The host program named `family` that shows each character a code of `spec`
    /// decodes to with the glyph the page draws for that code, advancing by its PDF width.
    ///
    /// The lowest code decoding to a character wins. Returns `None` for Type 0 and Type 3
    /// fonts and for a program the host program cannot be built from.
    ///
    /// # Errors
    /// Propagates font loading, character decoding and table writing failures.
    pub fn host_font_program(
        &self,
        spec: &PdfFontSpec,
        family: &str,
    ) -> Result<Option<HostFontProgram>, TextError> {
        let font = match spec {
            PdfFontSpec::Type1(font)
            | PdfFontSpec::MultipleMasterType1(font)
            | PdfFontSpec::TrueType(font) => font,
            PdfFontSpec::Type0(_) | PdfFontSpec::Type3(_) => return Ok(None),
        };
        let handle = self.load_pdf_font(spec.clone())?;
        let face = handle.primary().as_ref();
        let mut glyphs = BTreeMap::new();
        for code in u8::MIN..=u8::MAX {
            let decoded = decode_one_byte_mapping(
                &font.encoding,
                &font.metrics,
                font.to_unicode.as_deref(),
                code,
            )?;
            let UnicodeSequence::Scalar(character) = decoded.unicode else {
                continue;
            };
            let Some(glyph) = select_glyph(face, &decoded, handle.uses_substitute())
                .filter(|glyph| *glyph != GlyphId(0))
            else {
                continue;
            };
            glyphs.entry(character).or_insert(HostGlyph {
                character,
                glyph,
                advance: decoded.pdf_advance.x,
            });
        }
        let glyphs: Vec<HostGlyph> = glyphs.into_values().collect();
        Ok(HostFontProgram::new(handle.source(), family, &glyphs)?)
    }
}
