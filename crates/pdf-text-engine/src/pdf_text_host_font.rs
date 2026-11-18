//! The host font program of a PDF font: the program its glyphs are drawn from, with each
//! character mapped onto the glyph layout selects for the code drawn text encodes it as.

use std::sync::Arc;

use pdf_cmap::PdfCode;
use pdf_font::{
    GlyphId, PDF_GLYPH_SPACE_UNITS_PER_EM, PdfFontSpec,
    host_font_program::{HostFontProgram, HostGlyph},
    pdf_font_handle::PdfFontHandle,
};

use crate::{
    error::TextError,
    pdf_text_encoder::PdfTextEncoder,
    system::FontSystem,
    text::{PdfTextItem, PdfTextRun},
    text_style::TextStyle,
};

impl FontSystem {
    /// The host program named `family` that shows each character `spec` encodes with the
    /// glyph the page draws for its code, advancing by the code's PDF width.
    ///
    /// Characters take the codes of [`PdfTextEncoder`], so a host editing text in this
    /// program shows the glyphs drawn text shows, and lines take the font's line metrics,
    /// so its lines are spaced as drawn lines are. A `comb_cell` width, in thousandths of
    /// an em, spaces the glyphs one per cell. Returns `None` for Type 0 and Type 3
    /// fonts and for a program the host program cannot be built from.
    ///
    /// # Errors
    /// Propagates font loading, character decoding and table writing failures.
    pub fn host_font_program(
        &self,
        spec: Arc<PdfFontSpec>,
        family: &str,
        comb_cell: Option<f32>,
    ) -> Result<Option<HostFontProgram>, TextError> {
        if matches!(spec.as_ref(), PdfFontSpec::Type0(_) | PdfFontSpec::Type3(_)) {
            return Ok(None);
        }
        let encoder = PdfTextEncoder::new(&spec)?;
        let handle = self.load_pdf_font(spec)?;
        let glyphs: Vec<HostGlyph> = encoder
            .codes()
            .filter_map(|(character, code)| self.drawn_glyph(&handle, character, code))
            .collect();
        Ok(HostFontProgram::new(
            handle.source(),
            family,
            &glyphs,
            handle.line_metrics(),
            comb_cell,
        )?)
    }

    /// The glyph of `font`'s primary face that layout draws `code` with, mapped from
    /// `character`, with the code's PDF advance; `None` when layout draws it from another
    /// face or with the missing glyph.
    fn drawn_glyph(
        &self,
        font: &PdfFontHandle,
        character: char,
        code: PdfCode,
    ) -> Option<HostGlyph> {
        let items = [PdfTextItem::Text(code.bytes().collect::<Vec<u8>>().into())];
        let run = PdfTextRun {
            font,
            items: &items,
            // At a size of one glyph-space em, advances are in thousandths of an em.
            style: TextStyle {
                font_size: PDF_GLYPH_SPACE_UNITS_PER_EM,
                ..TextStyle::default()
            },
        };
        let layout = self.layout_pdf(&run).ok()?;
        let [glyph_run] = layout.runs.as_slice() else {
            return None;
        };
        let [positioned] = glyph_run.glyphs.as_slice() else {
            return None;
        };
        (glyph_run.face.id() == font.primary().id() && positioned.glyph_id != GlyphId(0)).then_some(
            HostGlyph {
                character,
                glyph: positioned.glyph_id,
                advance: layout.advance.x,
            },
        )
    }
}
