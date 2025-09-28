use pdf_graphics::pdf_path::PdfPath;

use crate::cff::{
    char_string_interpreter::CharStringOperator, char_string_interpreter_stack::CharStringStack,
    encoding::Encoding, error::CompactFontFormatError,
};

/// Represents a parsed CFF font program.
pub struct CffFontProgram {
    /// Parsed charstring programs per glyph. The outer `Vec` index is the glyph
    /// ID (GID). Each inner `Vec` is the ordered sequence of operators forming
    /// the glyph's charstring program.
    pub char_string_operators: Vec<Vec<CharStringOperator>>,
    /// The encoding mapping from 8-bit character codes (u8) appearing in a PDF
    /// content stream to glyph IDs (GIDs).
    pub encoding: Encoding,
}

impl CffFontProgram {
    /// Converts an 8-bit character code to a glyph ID (GID) based on the font's encoding.
    ///
    /// # Parameters
    ///
    /// - `code_point`: The 8-bit character code from a content stream.
    ///
    /// # Returns
    ///
    /// Returns `Some(u16)` containing the glyph ID if a mapping exists for the given
    /// `code_point`, or `None` if no mapping is found.
    fn code_to_gid(&self, code_point: u8) -> Option<u16> {
        match &self.encoding {
            Encoding::Standard(mapping) => {
                let code_point = u16::from(code_point);
                mapping.get(&code_point).cloned()
            }
        }
    }

    /// Renders a glyph for the given character code by interpreting its CFF charstring.
    ///
    /// This method first converts the character code to a Glyph ID (GID) using the
    /// font's encoding. It then retrieves the corresponding charstring program and
    /// executes its operators to construct the glyph's path.
    ///
    /// # Parameters
    ///
    /// - `char_code`: The 8-bit character code of the glyph to render.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(PdfPath))` if the glyph is successfully rendered.
    /// - `Ok(None)` if the `char_code` does not map to a valid glyph in the font.
    /// - `Err(CompactFontFormatError)` if an error occurs during the evaluation of the
    ///   charstring program.
    pub fn render_glyph(&self, char_code: u8) -> Result<Option<PdfPath>, CompactFontFormatError> {
        let Some(gid) = self.code_to_gid(char_code) else {
            return Ok(None);
        };

        let mut path = PdfPath::default();
        let mut eval_stack = CharStringStack::default();

        let Some(glyph_ops) = self.char_string_operators.get(usize::from(gid)) else {
            return Ok(None);
        };

        for op in glyph_ops {
            match op {
                CharStringOperator::Number(v) => {
                    eval_stack.push(*v)?;
                }
                CharStringOperator::Function(f) => f(&mut path, &mut eval_stack)?,
            }
        }

        Ok(Some(path))
    }
}
