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
    /// 1. **`FontFile3`** — stream inside the `/FontDescriptor`.
    ///    - `/Subtype /Type1C` or `/Subtype /CIDFontType0C`: raw CFF data,
    ///      wrapped into a minimal OpenType container so downstream renderers
    ///      (`skrifa` / `read-fonts`) can consume it.
    ///    - `/Subtype /OpenType`: already an OpenType font program, returned as-is.
    ///    - Any other or missing `/Subtype`: unsupported.
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
    /// - [`FontError::UnsupportedFontSubtype`] — unsupported `FontFile3` subtype
    ///   or a `FontFile` (classic Type 1) stream.
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

        // Path 1: FontFile3 stream with subtype-driven handling.
        if let Some(font_file3) = descriptor.get("FontFile3") {
            let stream = font_file3.try_stream(objects)?;
            let subtype = stream
                .dictionary
                .get("Subtype")
                .map(|obj| obj.try_str(objects))
                .transpose()?;

            let data = stream.data()?;

            return match subtype.as_deref() {
                Some("Type1C") | Some("CIDFontType0C") => build_cff_font(data.as_ref()),
                Some("OpenType") => Ok(data.into_owned()),
                Some(other) => Err(FontError::UnsupportedFontSubtype {
                    subtype: other.to_string(),
                }),
                None => Err(FontError::UnsupportedFontSubtype {
                    subtype: "FontFile3".to_string(),
                }),
            };
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{
        object_resolver::PassthroughResolver, object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::*;

    fn make_font_dict_with_font_file3(subtype: Option<&str>, bytes: Vec<u8>) -> Dictionary {
        let mut file3_stream_dict = BTreeMap::new();
        if let Some(subtype) = subtype {
            file3_stream_dict.insert(
                "Subtype".to_string(),
                ObjectVariant::Name(subtype.as_bytes().to_vec()),
            );
        }
        let font_file3_stream = StreamObject::new(
            10,
            0,
            Box::new(Dictionary::new(file3_stream_dict)),
            bytes,
            None,
        );

        let mut descriptor_dict = BTreeMap::new();
        descriptor_dict.insert(
            "FontFile3".to_string(),
            ObjectVariant::Stream(font_file3_stream),
        );

        let mut font_dict = BTreeMap::new();
        font_dict.insert(
            "FontDescriptor".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(descriptor_dict))),
        );
        Dictionary::new(font_dict)
    }

    #[test]
    fn font_file3_type1c_is_wrapped_into_opentype() {
        let cff_bytes = vec![1, 2, 3, 4, 5];
        let dict = make_font_dict_with_font_file3(Some("Type1C"), cff_bytes.clone());

        let parsed = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap();
        let expected = build_cff_font(&cff_bytes).unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn font_file3_opentype_is_returned_as_is() {
        let opentype_bytes = vec![0, 1, 2, 3, 4, 5];
        let dict = make_font_dict_with_font_file3(Some("OpenType"), opentype_bytes.clone());

        let parsed = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap();
        assert_eq!(parsed, opentype_bytes);
    }

    #[test]
    fn font_file3_unknown_subtype_is_unsupported() {
        let dict = make_font_dict_with_font_file3(Some("Type42"), vec![1, 2, 3]);

        let err = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap_err();
        assert_eq!(
            err,
            FontError::UnsupportedFontSubtype {
                subtype: "Type42".to_string()
            }
        );
    }

    #[test]
    fn font_file3_missing_subtype_is_unsupported() {
        let dict = make_font_dict_with_font_file3(None, vec![1, 2, 3]);

        let err = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap_err();
        assert_eq!(
            err,
            FontError::UnsupportedFontSubtype {
                subtype: "FontFile3".to_string()
            }
        );
    }
}
