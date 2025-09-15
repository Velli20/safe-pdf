// Type1 font renderer for pdf-canvas
// This is a stub for the Type1FontRenderer. Actual glyph rasterization is not implemented yet.

use crate::{canvas::Canvas, error::PdfCanvasError, text_renderer::TextRenderer};
use pdf_content_stream::pdf_operator_backend::PdfOperatorBackend;
use pdf_font::cff::reader::CffFontReader;
use pdf_font::type1_font::Type1Font;
use pdf_object::ObjectVariant;

pub(crate) struct Type1FontRenderer<'a, T: PdfOperatorBackend + Canvas> {
    /// The canvas backend where glyphs are drawn.
    _canvas: &'a mut T,
    pub font: &'a Type1Font,
}

impl<'a, T: PdfOperatorBackend + Canvas> Type1FontRenderer<'a, T> {
    pub fn new(canvas: &'a mut T, font: &'a Type1Font) -> Self {
        Type1FontRenderer {
            _canvas: canvas,
            font,
        }
    }
}

impl<T: PdfOperatorBackend + Canvas> TextRenderer for Type1FontRenderer<'_, T> {
    fn render_text(&mut self, text: &[u8]) -> Result<(), PdfCanvasError> {
        let Some(fd) = self.font.font_descriptor.as_ref() else {
            println!(
                "Type1FontRenderer: Missing FontDescriptor for '{}'",
                self.font.base_font
            );
            return Ok(());
        };
        let Some(font_file_obj) = fd.font_file.as_ref() else {
            println!(
                "Type1FontRenderer: Missing FontFile in FontDescriptor for '{}'",
                self.font.base_font
            );
            return Ok(());
        };
        println!(
            "Type1FontRenderer: Rendering text '{}'",
            String::from_utf8_lossy(text)
        );

        match font_file_obj {
            ObjectVariant::Stream(s) => {
                let program = CffFontReader::new(&s.data).read_font_program()?;

                // for d in program.operators.iter() {
                //     match d {
                //         DictToken::Number(n) => print!(" {} ", n),
                //         DictToken::Real(s) => print!(" (real {}) ", s),
                //         DictToken::Operator(op) => print!(" [op {:?}] ", op),
                //     }
                // }
            }
            _ => {
                println!("Type1FontRenderer: Font file is not a stream");
            }
        }

        Ok(())
    }
}
