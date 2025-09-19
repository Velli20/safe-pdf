// Type1 font renderer for pdf-canvas
// This is a stub for the Type1FontRenderer. Actual glyph rasterization is not implemented yet.

use crate::{canvas::Canvas, error::PdfCanvasError, text_renderer::TextRenderer};
use pdf_content_stream::pdf_operator_backend::PdfOperatorBackend;
use pdf_font::cff::reader::{CffFontReader, Charset};
use pdf_font::cff::standard_strings::STANDARD_STRINGS;
use pdf_font::type1_font::Type1Font;
use pdf_graphics::PathFillType;
use pdf_graphics::pdf_path::PdfPath;
use pdf_graphics::transform::{self, Transform};
use pdf_object::ObjectVariant;

pub(crate) struct Type1FontRenderer<'a, T: PdfOperatorBackend + Canvas> {
    /// The canvas backend where glyphs are drawn.
    canvas: &'a mut T,
    pub font: &'a Type1Font,
    /// The Current Transformation Matrix (CTM) at the time of rendering.
    _current_transform: Transform,
}

impl<'a, T: PdfOperatorBackend + Canvas> Type1FontRenderer<'a, T> {
    pub fn new(canvas: &'a mut T, font: &'a Type1Font, current_transform: Transform) -> Self {
        Type1FontRenderer {
            canvas,
            font,
            _current_transform: current_transform,
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

        let mut transform = transform::Transform::identity();
        transform.scale(0.3, 0.3);
        transform.translate(100.0, 200.0);

        // Note: `first_char` from the PDF font dictionary is unrelated to CFF glyph
        // indexing. CharStrings are addressed by glyph ID (GID), derived from the CFF
        // `charset` mapping. We therefore do not use `first_char` here.

        let ObjectVariant::Stream(s) = font_file_obj else {
            println!("Type1FontRenderer: Font file is not a stream");
            return Ok(());
        };
        let program = CffFontReader::new(&s.data).read_font_program()?;

        for u in text {
            let char_code = *u;
            let ch = char::from(*u);
            println!("Rendering glyph {} '{}'", char_code, ch);

            // 1) PDF code -> glyph name (very rough StandardEncoding fallback).
            // TODO: honor font.base_encoding and Encoding Differences when available.
            let glyph_name: Option<String> = match ch {
                ' ' => Some("space".to_string()),
                '!' => Some("exclam".to_string()),
                '"' => Some("quotedbl".to_string()),
                '#' => Some("numbersign".to_string()),
                '$' => Some("dollar".to_string()),
                '%' => Some("percent".to_string()),
                '&' => Some("ampersand".to_string()),
                '\'' => Some("quoteright".to_string()),
                '(' => Some("parenleft".to_string()),
                ')' => Some("parenright".to_string()),
                '*' => Some("asterisk".to_string()),
                '+' => Some("plus".to_string()),
                ',' => Some("comma".to_string()),
                '-' => Some("hyphen".to_string()),
                '.' => Some("period".to_string()),
                '/' => Some("slash".to_string()),
                '0' => Some("zero".to_string()),
                '1' => Some("one".to_string()),
                '2' => Some("two".to_string()),
                '3' => Some("three".to_string()),
                '4' => Some("four".to_string()),
                '5' => Some("five".to_string()),
                '6' => Some("six".to_string()),
                '7' => Some("seven".to_string()),
                '8' => Some("eight".to_string()),
                '9' => Some("nine".to_string()),
                ':' => Some("colon".to_string()),
                ';' => Some("semicolon".to_string()),
                '<' => Some("less".to_string()),
                '=' => Some("equal".to_string()),
                '>' => Some("greater".to_string()),
                '?' => Some("question".to_string()),
                '@' => Some("at".to_string()),
                'A'..='Z' | 'a'..='z' => Some(ch.to_string()),
                '[' => Some("bracketleft".to_string()),
                '\\' => Some("backslash".to_string()),
                ']' => Some("bracketright".to_string()),
                '^' => Some("asciicircum".to_string()),
                '_' => Some("underscore".to_string()),
                '`' => Some("grave".to_string()),
                '{' => Some("braceleft".to_string()),
                '|' => Some("bar".to_string()),
                '}' => Some("braceright".to_string()),
                '~' => Some("asciitilde".to_string()),
                _ => None,
            };

            // 2) glyph name -> CFF SID (STANDARD_STRINGS or string_index entries 391+)
            let sid: Option<u16> = if let Some(name) = glyph_name {
                // First search in standard strings
                if let Some(pos) = STANDARD_STRINGS.iter().position(|n| *n == name.as_str()) {
                    u16::try_from(pos).ok()
                } else {
                    // Then search in CFF string INDEX: SID = 391 + index
                    let sid_base = 391u16;
                    let idx = program
                        .string_index
                        .iter()
                        .position(|s| core::str::from_utf8(s).ok() == Some(name.as_str()));
                    idx.and_then(|i| u16::try_from(i).ok())
                        .and_then(|i_u16| sid_base.checked_add(i_u16))
                }
            } else {
                None
            };

            let Some(sid) = sid else {
                println!(
                    "Type1FontRenderer: Glyph name/SID not found for char code {}",
                    char_code
                );
                continue;
            };

            let mut path = PdfPath::default();

            // 3) SID -> GID using CFF charset. GID 0 is .notdef; charset covers GIDs 1..N-1
            let gid_opt: Option<usize> = match &program.charset {
                Charset::SID(sids) => sids
                    .iter()
                    .position(|s| s.0 == sid)
                    .and_then(|pos| pos.checked_add(1)),
                Charset::SIDRange(ranges) => {
                    let mut acc: usize = 0;
                    let mut found: Option<usize> = None;
                    for (first_sid, n_left) in ranges {
                        let start = first_sid.0;
                        // inclusive range length = n_left + 1, with checked math
                        let count_opt = usize::from(*n_left).checked_add(1);
                        let Some(count) = count_opt else {
                            break;
                        };
                        let start_u32 = u32::from(start);
                        let end_u32 = match u32::try_from(count)
                            .ok()
                            .and_then(|c| start_u32.checked_add(c))
                        {
                            Some(v) => v,
                            None => break,
                        };
                        let sid_u32 = u32::from(sid);
                        if sid_u32 >= start_u32 && sid_u32 < end_u32 {
                            if let Some(offset) = sid_u32
                                .checked_sub(start_u32)
                                .and_then(|d| usize::try_from(d).ok())
                            {
                                found = acc.checked_add(offset).and_then(|v| v.checked_add(1));
                            }
                            break;
                        }
                        acc = acc.saturating_add(count);
                    }
                    found
                }
                Charset::IsoAdobe | Charset::Expert | Charset::ExpertSubset => {
                    println!("Type1FontRenderer: Built-in charset ordering not yet supported");
                    None
                }
            };

            let Some(gid) = gid_opt else {
                println!(
                    "Type1FontRenderer: GID not found for char code {} (SID {})",
                    char_code, sid
                );
                continue;
            };

            if gid >= program.char_string_operators.len() {
                println!(
                    "Type1FontRenderer: GID {} out of bounds (charstrings len {})",
                    gid,
                    program.char_string_operators.len()
                );
                continue;
            }

            let glyph_ops = &program.char_string_operators[gid];
            println!("Type1FontRenderer: Glyph for char code {}:", char_code);
            for op in glyph_ops {
                op.call(&mut path);
            }
            path.transform(&transform);
            transform.translate(20.0, 0.0);

            self.canvas.fill_path(&path, PathFillType::Winding)?;
        }

        Ok(())
    }
}
