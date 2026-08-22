use std::collections::HashMap;

use pdf_cmap::ToUnicodeCMap;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{
    encoding::Encoding, error::FontError, flags::FontFlags, font_data::FontData,
    simple_font_glyph_map::SimpleFontGlyphWidthsMap, standard14::Standard14Font,
};

/// A TrueType font parsed from a PDF font dictionary.
pub struct TrueTypeFont {
    /// Font program bytes.
    ///
    /// Bundled fallback fonts borrow static data, while embedded fonts share
    /// the decoded PDF stream allocation.
    pub font_file: FontData,
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

pub(crate) struct TrueTypeFontProgram {
    pub(crate) font_file: FontData,
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
    /// Reads the embedded font program and optional `/Widths`, `/Encoding`, and
    /// `/ToUnicode` entries. Missing or unreadable programs remain errors so
    /// [`Font`](crate::font::Font) can apply whole-font fallback consistently.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FontError> {
        let program = Self::read_font_file(dictionary, objects)?;
        // Read the `/Widths` entry.
        let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?;

        // Read optional `/Encoding` entry — either a name (base encoding) or a
        // dictionary (with optional BaseEncoding + Differences).  Errors are
        // treated as absent encoding rather than propagated, since TrueType fonts
        // often omit or mis-specify this entry.
        let encoding = Encoding::from_dictionary(dictionary, objects)
            .ok()
            .flatten()
            .or_else(|| Self::default_simple_encoding(program.flags, None));

        let to_unicode = ToUnicodeCMap::from_dictionary(dictionary, objects)?;

        Ok(Self {
            font_file: program.font_file,
            widths,
            encoding,
            to_unicode,
            standard14: None,
            flags: program.flags,
        })
    }

    /// Creates a minimal `TrueTypeFont` from raw font bytes without PDF width,
    /// encoding, ToUnicode, or descriptor metadata.
    pub fn from_bytes(font_file: &'static [u8], standard14: Option<Standard14Font>) -> Self {
        Self {
            font_file: font_file.into(),
            widths: None,
            encoding: None,
            to_unicode: None,
            standard14,
            flags: FontFlags::empty(),
        }
    }

    /// Builds a synthetic simple font backed by one of the bundled Standard 14
    /// font programs.
    ///
    /// This constructor is intended for generated appearances and other
    /// in-memory fonts that need a stable font resource without parsing a PDF
    /// font dictionary.
    pub fn synthetic_standard14_font(standard14: Standard14Font) -> Self {
        Self {
            font_file: standard14.fallback_font_bytes().into(),
            widths: None,
            encoding: Some(Encoding::win_ansi()),
            to_unicode: None,
            standard14: Some(standard14),
            flags: FontFlags::NON_SYMBOLIC,
        }
    }
}

impl TrueTypeFont {
    /// Reads the embedded TrueType font file from a PDF font dictionary.
    ///
    /// This method expects the full font dictionary, looks up its `/FontDescriptor`
    /// entry, and then attempts to read the font data from the descriptor's
    /// `FontFile2` stream entry.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The font dictionary containing a `/FontDescriptor` entry.
    /// - `objects`: An object resolver for dereferencing indirect PDF objects.
    ///
    /// # Returns
    ///
    /// Returns the embedded font program or a [`FontError`] if the descriptor
    /// or `FontFile2` stream is missing or unreadable.
    pub(crate) fn read_font_file(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<TrueTypeFontProgram, FontError> {
        let flags = FontFlags::from_dictionary(dictionary, objects)?;

        let descriptor = dictionary
            .optional_dictionary(b"FontDescriptor", objects)?
            .ok_or(FontError::MissingFontFile)?;
        let stream = descriptor
            .optional_stream(b"FontFile2", objects)?
            .ok_or(FontError::MissingFontFile)?;

        Ok(TrueTypeFontProgram {
            font_file: FontData::shared(stream.shared_data()),
            flags,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, collections::BTreeMap};

    use pdf_object::{
        object_resolver::PassthroughResolver, object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::*;

    #[test]
    fn embedded_font_program_shares_stream_bytes() {
        let stream = StreamObject::new(
            1,
            0,
            Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new()),
            vec![1, 2, 3, 4],
        );
        let stream_bytes = stream.raw_data().as_ptr();
        let descriptor = Dictionary::new(BTreeMap::from([(
            b"FontFile2".to_vec(),
            ObjectVariant::Stream(stream),
        )]));
        let dictionary = Dictionary::new(BTreeMap::from([(
            b"FontDescriptor".to_vec(),
            ObjectVariant::Dictionary(descriptor),
        )]));

        let font = TrueTypeFont::from_dictionary(&dictionary, &PassthroughResolver)
            .expect("embedded TrueType font should parse");

        assert!(matches!(&font.font_file, FontData::Shared(_)));
        assert_eq!(font.font_file.as_ptr(), stream_bytes);
    }

    #[test]
    fn bundled_font_program_uses_static_bytes() {
        let fallback = Standard14Font::Helvetica.fallback_font_bytes();
        let fallback_bytes = fallback.as_ptr();
        let font = TrueTypeFont::from_bytes(fallback, Some(Standard14Font::Helvetica));

        assert!(matches!(&font.font_file, FontData::Static(_)));
        assert_eq!(font.font_file.as_ptr(), fallback_bytes);
    }

    #[test]
    fn raw_font_bytes_do_not_install_pdf_encoding_metadata() {
        let font = TrueTypeFont::from_bytes(
            Standard14Font::Helvetica.fallback_font_bytes(),
            Some(Standard14Font::Helvetica),
        );

        assert!(font.widths.is_none());
        assert!(font.encoding.is_none());
        assert!(font.to_unicode.is_none());
        assert!(font.flags.is_empty());
    }

    #[test]
    fn non_symbolic_simple_fonts_default_to_standard_encoding() {
        let encoding = TrueTypeFont::default_simple_encoding(FontFlags::NON_SYMBOLIC, None);

        assert_eq!(
            encoding
                .as_ref()
                .and_then(|encoding| encoding.names.get(65))
                .map(Cow::as_ref),
            Some(b"A".as_slice()),
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
            Some(b"R".as_slice()),
        );
    }
}
