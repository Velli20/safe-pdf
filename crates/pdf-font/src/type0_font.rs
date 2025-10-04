use pdf_object::{
    dictionary::Dictionary,
    error::ObjectError,
    object_collection::ObjectCollection,
    stream::StreamObject,
    traits::{FromDictionary, FromStreamObject},
};

use crate::{
    character_map::{CMapError, CharacterMap},
    font::FontEncoding,
    font_descriptor::{FontDescriptor, FontDescriptorError},
    glyph_widths_map::{GlyphWidthsMap, GlyphWidthsMapError},
};
use thiserror::Error;

/// Represents a PDF Type0 (composite) font, which references a CIDFont
/// for glyph definitions.
pub struct Type0Font {
    /// The default width for glyphs in the font.
    /// This is the `/DW` entry in the CIDFont dictionary.
    pub default_width: f32,
    /// The CIDFont subtype (CIDFontType0 or CIDFontType2).
    pub subtype: CidFontSubType,
    /// Optional font file containing embedded TrueType program.
    pub font_file: Option<StreamObject>,
    /// A map of individual glyph widths, overriding the default width for specific CIDs.
    /// This corresponds to the `/W` entry in the CIDFont dictionary.
    pub widths: Option<GlyphWidthsMap>,
    /// A stream defining a CMap that maps character codes to Unicode values.
    pub cmap: Option<CharacterMap>,
    /// Optional encoding information for simple fonts (Type1, TrueType).
    pub encoding: Option<FontEncoding>,
}

impl Type0Font {
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

/// Defines errors that can occur while reading a PDF objects.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum Type0FontError {
    #[error("FontDescriptor parsing error: {0}")]
    FontDescriptorError(#[from] FontDescriptorError),
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("GlyphWidthsMap parsing error: {0}")]
    GlyphWidthsMapError(#[from] GlyphWidthsMapError),
    #[error("Unsupported CIDFont subtype '{subtype}'")]
    UnsupportedCidFontSubtype { subtype: String },
    #[error("CMap parsing error: {0}")]
    CMapParse(#[from] CMapError),
    #[error("Invalid /DescendantFonts entry in Type0 font: {0}")]
    InvalidDescendantFonts(&'static str),
}

impl FromDictionary for Type0Font {
    const KEY: &'static str = "Font";

    type ResultType = Self;
    type ErrorType = Type0FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // Attempt to resolve the optional `/ToUnicode` CMap stream, which maps character codes to Unicode.
        // If present, parse it into a `CharacterMap`. If not present, set cmap to None.
        let cmap = dictionary
            .get("ToUnicode")
            .map(|obj| objects.resolve_stream(obj))
            .transpose()?
            .map(CharacterMap::from_stream_object)
            .transpose()?;

        // Attempt to extract the `/Encoding` entry, if present, and convert it to a `FontEncoding`.
        let encoding = dictionary
            .get("Encoding")
            .and_then(|v| v.as_str())
            .map(FontEncoding::from);

        // The `/DescendantFonts` array is required for Type0 fonts. Return an error if missing.
        let descendant_fonts_array = dictionary.get_or_err("DescendantFonts")?.try_array()?;
        if descendant_fonts_array.len() != 1 {
            return Err(Type0FontError::InvalidDescendantFonts(
                "Expected exactly one descendant font",
            ));
        }

        // The array must not be empty. Get the first element, which should reference the CIDFont dictionary.
        let cid_font_ref_val = descendant_fonts_array
            .first()
            .ok_or(Type0FontError::InvalidDescendantFonts("Array is empty"))?;

        // Resolve the CIDFont dictionary from the reference..
        let dictionary = objects.resolve_dictionary(cid_font_ref_val)?;

        // Determine the CIDFont subtype from the dictionary. Currently, only CIDFontType2
        // (TrueType Outlines) is supported.
        let subtype = match dictionary.get_or_err("Subtype")?.try_str()?.as_ref() {
            "CIDFontType2" => CidFontSubType::Type2,
            other => {
                return Err(Type0FontError::UnsupportedCidFontSubtype {
                    subtype: other.to_string(),
                });
            }
        };

        let default_width = dictionary
            .get("DW")
            .map(|dw| dw.as_number::<f32>())
            .transpose()?
            .unwrap_or(Self::DEFAULT_WIDTH);

        let widths_map = dictionary
            .get("W")
            .map(|obj| -> Result<GlyphWidthsMap, Type0FontError> {
                let resolved_obj = objects.resolve_object(obj)?.try_array()?;
                GlyphWidthsMap::from_array(resolved_obj).map_err(Type0FontError::from)
            })
            .transpose()?;

        // FontDescriptor must be an indirect reference according to the PDF spec.
        let font_file = if let Some(fd_obj) = dictionary.get("FontDescriptor") {
            let fd_dict = objects.resolve_dictionary(fd_obj)?;
            let FontDescriptor { font_file } = FontDescriptor::from_dictionary(fd_dict, objects)?;
            font_file
        } else {
            None
        };

        Ok(Self {
            default_width,
            subtype,
            font_file,
            widths: widths_map,
            cmap,
            encoding,
        })
    }
}
