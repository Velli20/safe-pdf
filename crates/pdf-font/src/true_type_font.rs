use std::{borrow::Cow, collections::HashMap};

use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

use crate::{
    encoding::{Encoding, FontEncoding},
    flags::FontFlags,
    font::FontError,
    simple_font_glyph_map::SimpleFontGlyphWidthsMap,
    standard14::Standard14Font,
    to_unicode_cmap::ToUnicodeCMap,
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
}

impl TrueTypeFont {
    /// Parses a TrueType font from a PDF font dictionary.
    ///
    /// Reads the embedded font program (or falls back to a bundled substitute),
    /// optional `/Widths`, `/Encoding`, and `/ToUnicode` entries.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FontError> {
        let font_file = Self::read_font_file(dictionary, objects)?;
        // Read the `/Widths` entry.
        let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?;

        // Read optional `/Encoding` entry — either a name (base encoding) or a
        // dictionary (with optional BaseEncoding + Differences).  Errors are
        // treated as absent encoding rather than propagated, since TrueType fonts
        // often omit or mis-specify this entry.
        let encoding: Option<Encoding> = dictionary.get("Encoding").and_then(|enc_obj| {
            let resolved = objects.resolve_object(enc_obj).ok()?;
            match resolved {
                ObjectVariant::Dictionary(d) => Encoding::from_dictionary(d, objects).ok(),
                _ => {
                    let base = FontEncoding::from(resolved.try_str(objects).ok()?);
                    Encoding::from_base_encoding(base).ok()
                }
            }
        });

        // Parse optional ToUnicode CMap stream.
        let to_unicode = dictionary
            .get("ToUnicode")
            .and_then(|e| e.try_stream(objects).ok())
            .and_then(|s| s.data().ok())
            .map(|data| ToUnicodeCMap::from_bytes(&data));

        Ok(Self {
            font_file,
            widths,
            encoding,
            to_unicode,
            standard14: None,
        })
    }

    /// Creates a minimal `TrueTypeFont` from raw font bytes with no
    /// widths, encoding, or ToUnicode map.
    ///
    /// Used for Standard 14 fallback fonts where the bundled bytes are
    /// `Cow::Borrowed` (zero-copy from `include_bytes!`).
    pub fn from_bytes(font_file: Cow<'static, [u8]>, standard14: Option<Standard14Font>) -> Self {
        Self {
            font_file,
            widths: None,
            encoding: None,
            to_unicode: None,
            standard14,
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
    /// Returns the font file bytes as a `Cow<'static, [u8]>` or an [`ObjectError`] if
    /// reading or decompressing the font stream fails or if the font dictionary or its
    /// `/FontDescriptor` entry is invalid.
    pub(crate) fn read_font_file(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Cow<'static, [u8]>, ObjectError> {
        let flags = if let Some(descriptor) = dictionary
            .get("FontDescriptor")
            .map(|obj| obj.try_dictionary(objects))
            .transpose()?
        {
            if let Some(stream) = descriptor
                .get("FontFile2")
                .map(|obj| obj.try_stream(objects))
                .transpose()?
            {
                return Ok(Cow::Owned(stream.data()?.to_vec()));
            }

            // Read Flags to determine fallback font style
            descriptor
                .get("Flags")
                .and_then(|obj| obj.try_number::<u32>(objects).ok())
                .map(FontFlags::from_bits_truncate)
                .unwrap_or_default()
        } else {
            FontFlags::empty()
        };

        Ok(Standard14Font::from(flags).fallback_font_bytes())
    }
}
