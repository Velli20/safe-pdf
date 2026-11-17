//! The host font program of a PDF font: the program its glyphs are drawn from, with each
//! character mapped onto the glyph layout selects for the code drawn text encodes it as.

use std::sync::Arc;

use pdf_font::{
    GlyphId, PdfFontSpec,
    host_font_program::{HostFontProgram, HostGlyph},
};

use crate::{
    error::TextError, pdf_text_decoder::decode_one_byte_mapping, pdf_text_encoder::PdfTextEncoder,
    pdf_text_layout::select_glyph, system::FontSystem,
};

impl FontSystem {
    /// The host program named `family` that shows each character `spec` encodes with the
    /// glyph the page draws for its code, advancing by the code's PDF width.
    ///
    /// Characters take the codes of [`PdfTextEncoder`], so a host editing text in this
    /// program shows the glyphs drawn text shows, and lines take the font's line metrics,
    /// so its lines are spaced as drawn lines are. Returns `None` for Type 0 and Type 3
    /// fonts and for a program the host program cannot be built from.
    ///
    /// # Errors
    /// Propagates font loading, character decoding and table writing failures.
    pub fn host_font_program(
        &self,
        spec: Arc<PdfFontSpec>,
        family: &str,
    ) -> Result<Option<HostFontProgram>, TextError> {
        let font = match spec.as_ref() {
            PdfFontSpec::Type1(font)
            | PdfFontSpec::MultipleMasterType1(font)
            | PdfFontSpec::TrueType(font) => font,
            PdfFontSpec::Type0(_) | PdfFontSpec::Type3(_) => return Ok(None),
        };
        let encoder = PdfTextEncoder::new(&spec)?;
        let handle = self.load_pdf_font(Arc::clone(&spec))?;
        let face = handle.primary().as_ref();
        let mut glyphs = Vec::new();
        for (character, code) in encoder.codes() {
            let Ok(code) = u8::try_from(code.value()) else {
                continue;
            };
            let decoded = decode_one_byte_mapping(
                &font.encoding,
                &font.metrics,
                font.to_unicode.as_deref(),
                code,
            )?;
            if let Some(glyph) = select_glyph(face, &decoded, handle.uses_substitute())
                .filter(|glyph| *glyph != GlyphId(0))
            {
                glyphs.push(HostGlyph {
                    character,
                    glyph,
                    advance: decoded.pdf_advance.x,
                });
            }
        }
        Ok(HostFontProgram::new(
            handle.source(),
            family,
            &glyphs,
            handle.line_metrics(),
        )?)
    }
}
