use std::{borrow::Cow, collections::HashMap};

use pdf_cmap::ToUnicodeCMap;
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    encoding::{Encoding, FontEncoding},
    error::FontError,
    fallback::fallback_program_from_dictionary,
    flags::FontFlags,
    simple_font_glyph_map::SimpleFontGlyphWidthsMap,
    standard14::Standard14Font,
};

/// A TrueType font parsed from a PDF font dictionary.
pub struct TrueTypeFont {
    /// Font program bytes.
    ///
    /// `Cow::Borrowed` for bundled Standard 14 fallback fonts (avoids copying
    /// the static `include_bytes!` data), `Cow::Owned` for fonts embedded in
    /// the PDF stream.
    pub font_file: Cow<'static, [u8]>,
    /// Widths for character codes.
    pub widths: Option<HashMap<u16, f32>>,
    /// Optional glyph name encoding, used as AGL fallback when ToUnicode is absent.
    pub encoding: Option<Encoding>,
    /// Parsed ToUnicode CMap for char-code → Unicode mapping.
    pub to_unicode: Option<ToUnicodeCMap>,
    /// Standard 14 identity when this font is a synthetic fallback selected
    /// from a Standard 14 `/BaseFont` name.
    pub standard14: Option<Standard14Font>,
    /// Font flags from the PDF font descriptor, if available.  Used to determine
    /// whether to apply the symbolic font fallback behavior for unmapped char codes.
    pub flags: FontFlags,
}

struct TrueTypeFontProgram {
    font_file: Cow<'static, [u8]>,
    standard14: Option<Standard14Font>,
    flags: FontFlags,
}

impl TrueTypeFont {
    fn default_simple_encoding(
        flags: FontFlags,
        standard14: Option<Standard14Font>,
    ) -> Option<Encoding> {
        if standard14.is_some()
            || flags.contains(FontFlags::NON_SYMBOLIC)
            || !flags.contains(FontFlags::SYMBOLIC)
        {
            Some(Encoding::default())
        } else {
            None
        }
    }

    /// Parses a TrueType font from a PDF font dictionary.
    ///
    /// Reads the embedded font program (or falls back to a bundled substitute),
    /// optional `/Widths`, `/Encoding`, and `/ToUnicode` entries.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FontError> {
        let program = Self::read_font_program(dictionary, objects)?;
        // Read the `/Widths` entry.
        let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?;

        // Read optional `/Encoding` entry — either a name (base encoding) or a
        // dictionary (with optional BaseEncoding + Differences).  Errors are
        // treated as absent encoding rather than propagated, since TrueType fonts
        // often omit or mis-specify this entry.
        let encoding: Option<Encoding> = dictionary
            .get("Encoding")
            .and_then(|enc_obj| {
                let resolved = objects.resolve_object(enc_obj).ok()?;
                match resolved {
                    ObjectVariant::Dictionary(d) => Encoding::from_dictionary(d, objects).ok(),
                    _ => {
                        let base = FontEncoding::from(resolved.try_str(objects).ok()?);
                        Encoding::from_base_encoding(base).ok()
                    }
                }
            })
            .or_else(|| Self::default_simple_encoding(program.flags, program.standard14));

        // Parse optional ToUnicode CMap stream.
        let to_unicode = dictionary
            .get("ToUnicode")
            .and_then(|e| e.try_stream(objects).ok())
            .and_then(|s| s.data().ok())
            .map(|data| ToUnicodeCMap::try_from(data.as_ref()))
            .transpose()?;

        Ok(Self {
            font_file: program.font_file,
            widths,
            encoding,
            to_unicode,
            standard14: program.standard14,
            flags: program.flags,
        })
    }

    /// Creates a minimal `TrueTypeFont` from raw font bytes with no
    /// widths or ToUnicode map.
    ///
    /// Used for Standard 14 fallback fonts where the bundled bytes are
    /// `Cow::Borrowed` (zero-copy from `include_bytes!`). Those fallback fonts
    /// behave like simple Type 1 fonts, so they default to StandardEncoding
    /// when the PDF omitted an explicit `/Encoding`.
    pub fn from_bytes(font_file: Cow<'static, [u8]>, standard14: Option<Standard14Font>) -> Self {
        Self {
            font_file,
            widths: None,
            encoding: Self::default_simple_encoding(FontFlags::empty(), standard14),
            to_unicode: None,
            standard14,
            flags: FontFlags::empty(),
        }
    }
}

impl TrueTypeFont {
    /// Reads the embedded TrueType font file from a PDF font dictionary.
    ///
    /// This method expects the full font dictionary, looks up its `/FontDescriptor`
    /// entry, and then attempts to read the font data from the descriptor's
    /// `FontFile2` stream entry. If no embedded font is present, it falls back to a
    /// bundled built-in TrueType font.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The font dictionary containing a `/FontDescriptor` entry.
    /// - `objects`: An object resolver for dereferencing indirect PDF objects.
    ///
    /// # Returns
    ///
    /// Returns the font file bytes as a `Cow<'static, [u8]>` or a [`FontError`] if
    /// reading or decompressing the font stream fails or if the font dictionary or its
    /// `/FontDescriptor` entry is invalid.
    pub(crate) fn read_font_file(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<(Cow<'static, [u8]>, FontFlags), FontError> {
        let program = Self::read_font_program(dictionary, objects)?;
        Ok((program.font_file, program.flags))
    }

    fn read_font_program(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<TrueTypeFontProgram, FontError> {
        if let Some(descriptor) = dictionary
            .get("FontDescriptor")
            .map(|obj| obj.try_dictionary(objects))
            .transpose()?
        {
            let flags = descriptor
                .get("Flags")
                .and_then(|obj| obj.try_number::<u32>(objects).ok())
                .map(FontFlags::from_bits_truncate)
                .unwrap_or_default();

            if let Some(stream) = descriptor
                .get("FontFile2")
                .map(|obj| obj.try_stream(objects))
                .transpose()?
            {
                return Ok(TrueTypeFontProgram {
                    font_file: Cow::Owned(stream.data()?.to_vec()),
                    standard14: None,
                    flags,
                });
            }
        }

        let fallback = fallback_program_from_dictionary(dictionary, objects)?;
        Ok(TrueTypeFontProgram {
            font_file: fallback.font_file,
            standard14: Some(fallback.standard14),
            flags: fallback.flags,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    #[test]
    fn standard14_fallback_fonts_default_to_standard_encoding() {
        let font = TrueTypeFont::from_bytes(
            Standard14Font::Helvetica.fallback_font_bytes(),
            Some(Standard14Font::Helvetica),
        );

        assert_eq!(
            font.encoding
                .as_ref()
                .and_then(|encoding| encoding.names.get(65))
                .map(std::borrow::Cow::as_ref),
            Some("A"),
        );
    }

    #[test]
    fn non_symbolic_simple_fonts_default_to_standard_encoding() {
        let encoding = TrueTypeFont::default_simple_encoding(FontFlags::NON_SYMBOLIC, None);

        assert_eq!(
            encoding
                .as_ref()
                .and_then(|encoding| encoding.names.get(65))
                .map(Cow::as_ref),
            Some("A"),
        );
    }

    #[test]
    fn symbolic_simple_fonts_without_fallback_keep_missing_encoding() {
        let encoding = TrueTypeFont::default_simple_encoding(FontFlags::SYMBOLIC, None);

        assert!(encoding.is_none());
    }

    #[test]
    fn contradictory_symbolic_and_non_symbolic_flags_prefer_standard_encoding() {
        let encoding = TrueTypeFont::default_simple_encoding(
            FontFlags::SYMBOLIC | FontFlags::NON_SYMBOLIC,
            None,
        );

        assert_eq!(
            encoding
                .as_ref()
                .and_then(|value| value.names.get(82))
                .map(Cow::as_ref),
            Some("R"),
        );
    }
}
