use std::collections::HashMap;

use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    cff_builder::build_cff_font,
    encoding::{Encoding, FontEncoding},
    font::FontError,
    simple_font_glyph_map::SimpleFontGlyphWidthsMap,
    to_unicode_cmap::ToUnicodeCMap,
};

/// Minimal, initial representation of a PDF Type1 font.
///
/// This focuses on dictionary-level metadata needed by higher layers
/// and defers actual glyph rendering or embedded program parsing.
pub struct Type1Font {
    /// A stream containing the font program.
    pub font_file: Vec<u8>,
    /// Widths map for character codes.
    pub widths: Option<HashMap<u16, f32>>,
    /// Optional encoding information.
    pub encoding: Encoding,
    /// Parsed ToUnicode CMap for char-code → Unicode mapping.
    pub to_unicode: Option<ToUnicodeCMap>,
}

impl Type1Font {
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FontError> {
        // Read embedded font file.
        let font_file = Self::read_font_file(dictionary, objects)?;

        // Read the `/Widths` entry.
        let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?;

        // Read optional `/Encoding` entry. This is either a name or a dictionary.
        let encoding = if let Some(enc_obj) = dictionary.get("Encoding") {
            let enc_obj = objects.resolve_object(enc_obj)?;
            match enc_obj {
                ObjectVariant::Dictionary(enc_dictionary) => {
                    Encoding::from_dictionary(enc_dictionary, objects)?
                }
                _ => {
                    let base = FontEncoding::from(enc_obj.try_str(objects)?);
                    Encoding::from_base_encoding(base)?
                }
            }
        } else {
            Encoding::default()
        };

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
        })
    }
}

impl Type1Font {
    /// Reads and processes the embedded Type 1 font file from a PDF font dictionary.
    ///
    /// Tries the following sources in order:
    ///
    /// 1. **`FontFile3`** — a CFF (Compact Font Format) stream inside the
    ///    `/FontDescriptor`.  The raw CFF data is wrapped into a minimal
    ///    OpenType container so downstream renderers (`skrifa` / `read-fonts`)
    ///    can consume it.
    /// 2. **`FontFile`** — a classic PostScript Type 1 font program.
    ///    Currently unsupported; returns [`FontError::UnsupportedFontSubtype`].
    ///
    /// If neither stream is present (or there is no `/FontDescriptor` at all),
    /// returns [`FontError::MissingFontFile`].  The caller (`Font::from_dictionary`)
    /// catches that sentinel and falls back to a bundled Standard 14 substitute.
    ///
    /// # Errors
    ///
    /// - [`FontError::MissingFontFile`] — no embedded font program found.
    /// - [`FontError::UnsupportedFontSubtype`] — a `FontFile` (classic Type 1)
    ///   stream was found but the format is not yet supported.
    /// - Any [`FontError`] propagated from stream decompression or CFF building.
    pub(crate) fn read_font_file(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<u8>, FontError> {
        let descriptor = dictionary
            .get("FontDescriptor")
            .map(|obj| obj.try_dictionary(objects))
            .transpose()?;

        let Some(descriptor) = descriptor else {
            return Err(FontError::MissingFontFile);
        };

        // Path 1: CFF data in FontFile3.
        if let Some(font_file3) = descriptor.get("FontFile3") {
            let stream = font_file3.try_stream(objects)?;
            return build_cff_font(stream.data()?.as_ref());
        }

        // Path 2: classic Type 1 in FontFile (not yet supported).
        if descriptor.get("FontFile").is_some() {
            return Err(FontError::UnsupportedFontSubtype {
                subtype: "FontFile".to_string(),
            });
        }

        // No embedded font program at all.
        Err(FontError::MissingFontFile)
    }
}
