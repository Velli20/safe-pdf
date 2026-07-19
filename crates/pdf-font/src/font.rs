use std::borrow::Cow;

use pdf_cmap::ToUnicodeCMap;
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};
use read_fonts::TableProvider;
use skrifa::{FontRef, MetadataProvider};

use crate::{
    char_vec::CharVec,
    error::FontError,
    fallback::{
        fallback_true_type_from_dictionary, fallback_true_type_from_dictionary_best_effort,
    },
    glyph_name_to_unicode::glyph_name_to_unicode,
    standard14::Standard14Font,
    true_type_font::TrueTypeFont,
    type0_font::Type0Font,
    type1_font::Type1Font,
    type3_font::Type3Font,
};

/// Represents a font object in a PDF document.
pub enum Font {
    /// A CIDFont used as a descendant font in a Type0 font.
    Type0(Type0Font),
    /// A classic PostScript font.
    Type1(Type1Font),
    /// A type 3 font with glyphs defined by PDF content streams.
    Type3(Type3Font),
    /// A TrueType font.
    TrueType(TrueTypeFont),
}

impl Font {
    pub const KEY: &'static str = "Font";

    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Font, FontError> {
        // Determine the font subtype from the dictionary.
        let subtype = dictionary.required_str("Subtype", objects)?;
        match subtype {
            "Type0" => {
                let type0_font = Type0Font::from_dictionary(dictionary, objects)?;
                Ok(Font::Type0(type0_font))
            }
            "Type1" => match Type1Font::from_dictionary(dictionary, objects) {
                Err(FontError::MissingFontFile) => Ok(Font::TrueType(
                    fallback_true_type_from_dictionary(dictionary, objects)?,
                )),
                Ok(type1_font) => Ok(Font::Type1(type1_font)),
                Err(e) => Err(e),
            },
            "Type3" => {
                let type3_font = Type3Font::from_dictionary(dictionary, objects, id_allocator)?;
                Ok(Font::Type3(type3_font))
            }
            "TrueType" => {
                let tt_font = TrueTypeFont::from_dictionary(dictionary, objects)?;
                Ok(Font::TrueType(tt_font))
            }
            other => Err(FontError::UnsupportedFontSubtype {
                subtype: other.to_string(),
            }),
        }
    }

    /// Build a minimal Standard 14-backed fallback font for best-effort
    /// resource recovery.
    ///
    /// This is intentionally narrower than the normal font parsing path:
    /// once the original font dictionary has already failed to parse, this
    /// fallback does not attempt to preserve `/Widths`, `/Encoding`, or
    /// `/ToUnicode` data from that failed font. The synthetic font keeps only
    /// the bundled fallback program and the selected Standard 14 identity.
    ///
    /// Callers should use this only at higher-level recovery boundaries, such
    /// as page resource loading, where replacing an unreadable font is better
    /// than aborting the entire resource dictionary.
    pub fn fallback_from_dictionary_best_effort(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Self {
        Self::TrueType(fallback_true_type_from_dictionary_best_effort(
            dictionary, objects,
        ))
    }
}

impl Font {
    /// Returns the Standard 14 identity when this font is backed by the
    /// synthetic Standard 14 fallback path.
    pub fn as_standard14(&self) -> Option<Standard14Font> {
        match self {
            Font::TrueType(font) => font.standard14,
            _ => None,
        }
    }

    /// Returns a glyph width in PDF glyph-space units (1/1000 em) for simple
    /// and CID fonts when available.
    ///
    /// For Type0 fonts, this returns the font's default width when explicit
    /// width data is missing.
    pub fn glyph_width(&self, char_code: u16) -> Option<f32> {
        match self {
            Font::Type0(font) => {
                if let Some(w) = &font.widths {
                    return w.get_width(char_code).or(Some(font.default_width));
                }
                Some(font.default_width)
            }
            Font::TrueType(font) => font
                .widths
                .as_ref()
                .and_then(|w| w.get(&char_code).copied()),
            Font::Type1(font) => font
                .widths
                .as_ref()
                .and_then(|w| w.get(&char_code).copied()),
            _ => None,
        }
    }

    /// Measures encoded text in user-space units for a given font size.
    ///
    /// Widths come from the parsed PDF font width data when available. Missing
    /// simple-font widths fall back to 500 glyph-space units, matching a stable
    /// half-em approximation for layout decisions.
    pub fn encoded_text_width(&self, text: &[u8], font_size: f32) -> f32 {
        const DEFAULT_SIMPLE_WIDTH: f32 = 500.0;
        const GLYPH_SPACE_UNITS: f32 = 1000.0;

        let glyph_width_sum = match self {
            Font::Type0(font) => font
                .decode_bytes_to_cids(text)
                .into_iter()
                .map(|cid| self.glyph_width(cid).unwrap_or(DEFAULT_SIMPLE_WIDTH))
                .sum::<f32>(),
            _ => text
                .iter()
                .copied()
                .map(u16::from)
                .map(|code| {
                    self.glyph_width(code)
                        .or_else(|| self.open_type_glyph_width(code))
                        .unwrap_or(DEFAULT_SIMPLE_WIDTH)
                })
                .sum::<f32>(),
        };

        glyph_width_sum / GLYPH_SPACE_UNITS * font_size
    }

    fn open_type_glyph_width(&self, char_code: u16) -> Option<f32> {
        let Font::TrueType(font) = self else {
            return None;
        };
        let font_ref = FontRef::new(font.font_file.as_ref()).ok()?;
        let unicode = self.char_to_unicode(char_code)?;
        let glyph_id = font_ref.charmap().map(unicode)?;
        let advance = font_ref.hmtx().ok()?.advance(glyph_id)?;
        let units_per_em = font_ref.head().ok()?.units_per_em();
        if units_per_em == 0 {
            return None;
        }
        Some(f32::from(advance) / f32::from(units_per_em) * 1000.0)
    }

    pub fn glyph_name(&self, char_code: u16) -> Option<&str> {
        let index = usize::from(char_code);
        match self {
            Font::Type1(font) => font.encoding.names.get(index).map(Cow::as_ref),
            Font::Type3(font) => font
                .encoding
                .as_ref()
                .and_then(|enc| enc.names.get(index).map(Cow::as_ref)),
            Font::TrueType(font) => font
                .encoding
                .as_ref()
                .and_then(|enc| enc.names.get(index).map(Cow::as_ref)),
            _ => None,
        }
    }

    /// Map a PDF character code to all of its Unicode scalar values.
    ///
    /// Resolution order:
    /// 1. ToUnicode CMap — returns the full slice (handles ligatures such as "fi"
    ///    mapped to `['f','i']`).
    /// 2. Glyph name → Adobe Glyph List (Type1 / Type3 / TrueType with encodings).
    /// 3. Type0/CID reverse-cmap fallback (Identity-H/V fonts without ToUnicode).
    ///
    /// Returns an empty [`CharVec`] when no mapping is found.
    pub fn chars_to_unicode(&self, char_code: u16) -> CharVec {
        // Priority 1: ToUnicode CMap
        let to_unicode: Option<&ToUnicodeCMap> = match self {
            Font::Type0(f) => f.to_unicode.as_ref(),
            Font::Type1(f) => f.to_unicode.as_ref(),
            Font::TrueType(f) => f.to_unicode.as_ref(),
            Font::Type3(f) => f.to_unicode.as_ref(),
        };
        if let Some(chars) = to_unicode.and_then(|m| m.map_char_code(char_code))
            && !chars.is_empty()
        {
            return CharVec::from_slice(chars);
        }

        // Priority 2: glyph name → AGL
        if let Some(name) = self.glyph_name(char_code)
            && let Some(c) = glyph_name_to_unicode(name)
        {
            return CharVec::from(c);
        }

        // Priority 3: Type0 reverse-cmap (Identity-H/V without ToUnicode)
        if let Font::Type0(f) = self
            && let Some(map) = &f.glyph_to_unicode
            && let Some(&c) = map.get(&char_code)
        {
            return CharVec::from(c);
        }

        CharVec::new()
    }

    /// Map a PDF character code to its first Unicode scalar value.
    ///
    /// For ligatures that map to multiple code points, only the first is returned.
    /// Prefer [`chars_to_unicode`](Self::chars_to_unicode) when full coverage is needed.
    pub fn char_to_unicode(&self, char_code: u16) -> Option<char> {
        self.chars_to_unicode(char_code).into_iter().next().copied()
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use pdf_cmap::Type0EncodingCMap;

    use super::*;
    use crate::{encoding::Encoding, flags::FontFlags, true_type_font::TrueTypeFont};

    #[test]
    fn test_truetype_encoding_fallback() {
        // Build a TrueType font with a minimal encoding that maps char code 65
        // to the glyph name "A".  The AGL single-char rule maps "A" → U+0041.
        let names: Vec<Cow<'static, str>> = (0..256)
            .map(|index| {
                if index == 65 {
                    Cow::Borrowed("A")
                } else {
                    Cow::Borrowed(".notdef")
                }
            })
            .collect();
        let enc = Encoding { names };
        let font = Font::TrueType(TrueTypeFont {
            font_file: Cow::Owned(vec![]),
            widths: None,
            encoding: Some(enc),
            to_unicode: None,
            standard14: None,
            flags: FontFlags::empty(),
        });
        assert_eq!(font.char_to_unicode(65), Some('A'));
        assert_eq!(&*font.chars_to_unicode(65), ['A'].as_slice());
    }

    #[test]
    fn test_ligature_chars_to_unicode() {
        // ToUnicode CMap maps char 1 → fi (U+FB01) + fl (U+FB02) as a ligature pair.
        let cmap_data = b"beginbfchar\n<01> <FB01FB02>\nendbfchar\n";
        let cmap = ToUnicodeCMap::try_from(cmap_data.as_slice()).unwrap();
        let font = Font::TrueType(TrueTypeFont {
            font_file: Cow::Owned(vec![]),
            widths: None,
            encoding: None,
            to_unicode: Some(cmap),
            standard14: None,
            flags: FontFlags::empty(),
        });
        assert_eq!(
            &*font.chars_to_unicode(1),
            ['\u{FB01}', '\u{FB02}'].as_slice()
        );
        // char_to_unicode returns only the first character
        assert_eq!(font.char_to_unicode(1), Some('\u{FB01}'));
    }

    #[test]
    fn test_cmap_parsers_share_comment_whitespace_and_hex_rules() {
        let type0_data = br#"
        begincmap
        /WMode 0 def
        1 begincodespacerange
        <01> % comment between tokens
        <F>
        endcodespacerange
        1 begincidchar
        <01> 9
        endcidchar
        endcmap
        "#;
        let type0 = Type0EncodingCMap::from_bytes(type0_data).unwrap();
        assert_eq!(type0.decode(&[0x01, 0xF0]), vec![9, 0]);

        let to_unicode_data = br#"
        beginbfchar
        <01> % comment between tokens
        <041>
        endbfchar
        "#;
        let cmap = ToUnicodeCMap::try_from(to_unicode_data.as_slice()).unwrap();
        assert_eq!(cmap.map_char_code(0x01), Some(['\u{0410}'].as_slice()));
    }

    #[test]
    fn test_type0_glyph_to_unicode_fallback() {
        use std::collections::HashMap;

        use crate::type0_font::{CidFontSubType, Type0Font, Type0FontProgramFormat};

        let mut glyph_map: HashMap<u16, char> = HashMap::new();
        glyph_map.insert(65u16, 'A');
        let font = Font::Type0(Type0Font {
            subtype: CidFontSubType::Type2,
            program_format: Type0FontProgramFormat::TrueType {
                cid_to_unicode: false,
            },
            font_file: vec![],
            type1_program_format: None,
            widths: None,
            encoding: None,
            default_width: 1000.0,
            to_unicode: None,
            glyph_to_unicode: Some(glyph_map),
        });
        assert_eq!(&*font.chars_to_unicode(65), ['A'].as_slice());
        assert_eq!(font.char_to_unicode(65), Some('A'));
    }
}
