use std::collections::HashMap;

use pdf_object::{
    ObjectVariant, dictionary::Dictionary, object_resolver::ObjectResolver, traits::FromDictionary,
};

use crate::{
    cff_builder::build_cff_font,
    encoding::{Encoding, FontEncoding},
    font::FontError,
    simple_font_glyph_map::SimpleFontGlyphWidthsMap,
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
}

impl FromDictionary for Type1Font {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self::ResultType, Self::ErrorType> {
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

        Ok(Self {
            font_file,
            widths,
            encoding,
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
