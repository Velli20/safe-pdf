use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use crate::{
    font_descriptor::{FontDescriptor, FontDescriptorError},
    glyph_widths_map::{GlyphWidthsMap, GlyphWidthsMapError},
};
use thiserror::Error;

/// Represents a Character Identifier (CID) font.
///
/// CID-keyed fonts are a sophisticated type of font that can handle large character sets,
/// such as those required for East Asian languages. They define glyphs by character identifiers (CIDs)
/// rather than by character codes, providing a flexible way to manage complex typography.
pub struct CharacterIdentifierFont {
    /// The default width for glyphs in the font.
    /// This is the `/DW` entry in the CIDFont dictionary.
    pub default_width: f32,
    /// The CIDFont subtype (CIDFontType0 or CIDFontType2).
    pub subtype: CidFontSubType,
    /// The font descriptor for this font, providing detailed metrics and style information.
    pub descriptor: FontDescriptor,
    /// A map of individual glyph widths, overriding the default width for specific CIDs.
    /// This corresponds to the `/W` entry in the CIDFont dictionary.
    pub widths: Option<GlyphWidthsMap>,
}

impl CharacterIdentifierFont {
    /// Default value for the `/DW` entry, if not present in the font dictionary.
    const DEFAULT_WIDTH: f32 = 1000.0;
}

/// CIDFont subtypes supported by the parser.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CidFontSubType {
    /// Type 1/CFF based CID-keyed font
    Type0,
    /// TrueType based CID-keyed font
    Type2,
}

impl std::fmt::Display for CidFontSubType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CidFontSubType::Type0 => write!(f, "/CIDFontType0"),
            CidFontSubType::Type2 => write!(f, "/CIDFontType2"),
        }
    }
}

/// Defines errors that can occur while reading a PDF objects.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum CidFontError {
    #[error("FontDescriptor parsing error: {0}")]
    FontDescriptorError(#[from] FontDescriptorError),
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("GlyphWidthsMap parsing error: {0}")]
    GlyphWidthsMapError(#[from] GlyphWidthsMapError),
    #[error("Unsupported CIDFont subtype '{subtype}'")]
    UnsupportedCidFontSubtype { subtype: String },
}

impl FromDictionary for CharacterIdentifierFont {
    const KEY: &'static str = "Font";

    type ResultType = Self;
    type ErrorType = CidFontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let default_width = dictionary
            .get("DW")
            .map(|dw| dw.as_number::<f32>())
            .transpose()?
            .unwrap_or(Self::DEFAULT_WIDTH);

        let widths_map = dictionary
            .get("W")
            .map(|obj| -> Result<GlyphWidthsMap, CidFontError> {
                let arr = obj.try_array()?;
                GlyphWidthsMap::from_array(arr).map_err(CidFontError::from)
            })
            .transpose()?;

        let subtype = match dictionary.get_or_err("Subtype")?.try_str()?.as_ref() {
            "CIDFontType0" => CidFontSubType::Type0,
            "CIDFontType2" => CidFontSubType::Type2,
            other => {
                return Err(CidFontError::UnsupportedCidFontSubtype {
                    subtype: other.to_string(),
                });
            }
        };

        // FontDescriptor must be an indirect reference according to the PDF spec.
        let desc_dict = objects.resolve_dictionary(dictionary.get_or_err("FontDescriptor")?)?;
        let descriptor = FontDescriptor::from_dictionary(desc_dict, objects)?;

        Ok(Self {
            default_width,
            subtype,
            descriptor,
            widths: widths_map,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::ObjectVariant;

    use super::*;

    fn make_min_font_descriptor_dict() -> Dictionary {
        let mut d = BTreeMap::new();
        d.insert("Ascent".into(), Box::new(ObjectVariant::Integer(800)));
        d.insert("Descent".into(), Box::new(ObjectVariant::Integer(-200)));
        d.insert("CapHeight".into(), Box::new(ObjectVariant::Integer(700)));
        d.insert("Flags".into(), Box::new(ObjectVariant::Integer(32)));
        d.insert(
            "FontBBox".into(),
            Box::new(ObjectVariant::Array(vec![
                ObjectVariant::Integer(-50),
                ObjectVariant::Integer(-200),
                ObjectVariant::Integer(1000),
                ObjectVariant::Integer(900),
            ])),
        );
        d.insert(
            "FontName".into(),
            Box::new(ObjectVariant::Name("TestFont".into())),
        );
        d.insert("ItalicAngle".into(), Box::new(ObjectVariant::Integer(0)));
        d.insert("StemV".into(), Box::new(ObjectVariant::Integer(80)));
        Dictionary::new(d)
    }

    fn make_cidfont_dict(subtype: &str) -> Dictionary {
        let mut d = BTreeMap::new();
        d.insert(
            "Subtype".into(),
            Box::new(ObjectVariant::Name(subtype.into())),
        );
        d.insert("DW".into(), Box::new(ObjectVariant::Integer(1000)));
        // Simple W map: 0 [500 600]
        d.insert(
            "W".into(),
            Box::new(ObjectVariant::Array(vec![
                ObjectVariant::Integer(0),
                ObjectVariant::Array(vec![
                    ObjectVariant::Integer(500),
                    ObjectVariant::Integer(600),
                ]),
            ])),
        );
        // Inline FontDescriptor dictionary
        d.insert(
            "FontDescriptor".into(),
            Box::new(ObjectVariant::Dictionary(std::rc::Rc::new(
                make_min_font_descriptor_dict(),
            ))),
        );
        Dictionary::new(d)
    }

    #[test]
    fn parses_cidfont_type2() {
        let dict = make_cidfont_dict("CIDFontType2");
        let objects = ObjectCollection::default();
        let cid = CharacterIdentifierFont::from_dictionary(&dict, &objects).unwrap();
        assert_eq!(cid.default_width, 1000.0);
        assert_eq!(cid.subtype, CidFontSubType::Type2);
        assert!(cid.widths.is_some());
        assert!(cid.widths.as_ref().unwrap().get_width(0).is_some());
        assert!(cid.widths.as_ref().unwrap().get_width(1).is_some());
    }

    #[test]
    fn parses_cidfont_type0() {
        let dict = make_cidfont_dict("CIDFontType0");
        let objects = ObjectCollection::default();
        let cid = CharacterIdentifierFont::from_dictionary(&dict, &objects).unwrap();
        assert_eq!(cid.default_width, 1000.0);
        assert_eq!(cid.subtype, CidFontSubType::Type0);
        assert!(cid.widths.is_some());
    }

    #[test]
    fn rejects_unknown_cidfont_subtype() {
        let dict = make_cidfont_dict("CIDFontType9");
        let objects = ObjectCollection::default();
        let res = CharacterIdentifierFont::from_dictionary(&dict, &objects);
        assert!(matches!(
            res,
            Err(CidFontError::UnsupportedCidFontSubtype { .. })
        ));
    }
}
