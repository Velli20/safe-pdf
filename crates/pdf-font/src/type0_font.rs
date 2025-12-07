use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use crate::{
    cff_builder::build_cff_font,
    encoding::FontEncoding,
    font::FontError,
    font_descriptor::{FontDescriptor, FontDescriptorError},
    glyph_widths_map::{GlyphWidthsMap, GlyphWidthsMapError},
};
use thiserror::Error;

/// Represents a PDF Type0 (composite) font, which references a CIDFont
/// for glyph definitions.
pub struct Type0Font {
    /// The CIDFont subtype (CIDFontType0 or CIDFontType2).
    pub subtype: CidFontSubType,
    /// Font file containing embedded font data.
    pub font_file: Vec<u8>,
    /// A map of individual glyph widths, overriding the default width for specific CIDs.
    /// This corresponds to the `/W` entry in the CIDFont dictionary.
    pub widths: Option<GlyphWidthsMap>,
    /// Optional encoding information.
    pub encoding: Option<FontEncoding>,
    /// The default width for glyphs in the font.
    /// This is the `/DW` entry in the CIDFont dictionary.
    pub(crate) default_width: f32,
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
    #[error("Invalid /DescendantFonts entry in Type0 font: {0}")]
    InvalidDescendantFonts(&'static str),
}

impl FromDictionary for Type0Font {
    const KEY: &'static str = "Font";

    type ResultType = Self;
    type ErrorType = FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // Extract the optional `/Encoding` entry which specifies the CMap used to map
        // character codes to CIDs. Common values include "Identity-H" and "Identity-V".
        let encoding = dictionary
            .get("Encoding")
            .and_then(|v| v.as_str())
            .map(FontEncoding::from);

        // Per PDF spec, the `/DescendantFonts` array
        // must contain exactly one CIDFont reference. This single descendant provides
        // the actual glyph descriptions for the composite font.
        let descendant_fonts_array = dictionary
            .get_or_err("DescendantFonts")?
            .try_array(objects)?;
        if descendant_fonts_array.len() != 1 {
            return Err(Type0FontError::InvalidDescendantFonts(
                "Expected exactly one descendant font",
            )
            .into());
        }

        // Retrieve the sole CIDFont dictionary reference from the array.
        let cid_font_ref_val = descendant_fonts_array
            .first()
            .ok_or(Type0FontError::InvalidDescendantFonts("Array is empty"))?;

        // Resolve the indirect reference to obtain the CIDFont dictionary.
        let dictionary = objects.resolve_dictionary(cid_font_ref_val)?;

        // Determine the CIDFont subtype which dictates how glyph data is stored:
        // - CIDFontType0: Uses CFF (Compact Font Format) glyph descriptions.
        // - CIDFontType2: Uses TrueType glyph descriptions.
        let subtype = match dictionary.get_or_err("Subtype")?.try_str()?.as_ref() {
            "CIDFontType0" => CidFontSubType::Type0,
            "CIDFontType2" => CidFontSubType::Type2,
            other => {
                return Err(Type0FontError::UnsupportedCidFontSubtype {
                    subtype: other.to_string(),
                }
                .into());
            }
        };

        // The `/DW` (default width) entry specifies the default glyph width in glyph space
        // units (typically 1/1000 of a unit).
        let default_width = dictionary
            .get("DW")
            .map(|dw| dw.as_number::<f32>())
            .transpose()?
            .unwrap_or(Self::DEFAULT_WIDTH);

        // The `/W` array provides individual glyph widths that override the
        // default width for specific CIDs.
        let widths_map = dictionary
            .get("W")
            .map(|obj| -> Result<GlyphWidthsMap, Type0FontError> {
                let resolved_obj = objects.resolve_object(obj)?.try_array(objects)?;
                GlyphWidthsMap::from_array(resolved_obj).map_err(Type0FontError::from)
            })
            .transpose()?;

        // The `/FontDescriptor` dictionary contains font metrics and the embedded font
        // program (via `/FontFile`, `/FontFile2`, or `/FontFile3` entries).
        let descriptor = objects.resolve_dictionary(dictionary.get_or_err("FontDescriptor")?)?;
        let font_file = FontDescriptor::from_dictionary(descriptor, objects)?;

        let font_file = font_file.data.as_slice();

        // Process the embedded font data based on the CIDFont subtype:
        // - Type0 (CFF): Rebuild as a standalone CFF font for rendering libraries.
        // - Type2 (TrueType): Use the raw TrueType data directly.
        let font_file = match subtype {
            CidFontSubType::Type0 => build_cff_font(font_file)?,
            CidFontSubType::Type2 => font_file.to_vec(),
        };

        Ok(Self {
            subtype,
            font_file,
            widths: widths_map,
            encoding,
            default_width,
        })
    }
}
