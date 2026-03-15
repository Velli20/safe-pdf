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
    /// This method extracts the font program from the `FontFile3` stream within
    /// the font descriptor and converts it into a valid CFF (Compact Font Format)
    /// font structure.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The font dictionary containing the `FontDescriptor` reference.
    /// - `objects`: An object resolver for dereferencing indirect PDF objects.
    ///
    /// # Returns
    ///
    /// Returns the processed CFF font data as a `Vec<u8>` or a [`FontError`].
    pub(crate) fn read_font_file(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<u8>, FontError> {
        // Read embedded font file.
        let font_file = dictionary
            .get_or_err("FontDescriptor")?
            .try_dictionary(objects)?
            .get_or_err("FontFile3")?
            .try_stream(objects)?
            .data()?;

        // Build CFF font from the font file stream.
        build_cff_font(&font_file)
    }
}
