use std::collections::HashMap;

use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};
use read_fonts::{FontRef, TableProvider};

use crate::{
    encoding::FontEncoding, error::FontError, glyph_widths_map::GlyphWidthsMap,
    to_unicode_cmap::ToUnicodeCMap, true_type_font::TrueTypeFont, type1_font::Type1Font,
};

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
    /// Parsed ToUnicode CMap for char-code → Unicode mapping.
    pub to_unicode: Option<ToUnicodeCMap>,
    /// Reverse Unicode cmap built from the embedded font's cmap tables.
    ///
    /// Only populated for Identity-H/V encoded fonts that lack a ToUnicode
    /// stream.  Maps glyph ID (= CID = char code for Identity encoding) to
    /// the Unicode scalar value found in the font's best cmap subtable.
    pub glyph_to_unicode: Option<HashMap<u16, char>>,
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

impl Type0Font {
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FontError> {
        // Extract the optional `/Encoding` entry which specifies the CMap used to map
        // character codes to CIDs. Common values include "Identity-H" and "Identity-V".
        let encoding = dictionary
            .get("Encoding")
            .map(|v| v.try_str(objects))
            .transpose()?
            .map(FontEncoding::from);

        // Parse optional ToUnicode CMap from the top-level Type0 font dictionary.
        // (Must be read before `dictionary` is shadowed by the descendant dict below.)
        let to_unicode = dictionary
            .get("ToUnicode")
            .and_then(|e| e.try_stream(objects).ok())
            .and_then(|s| s.data().ok())
            .map(|data| ToUnicodeCMap::from_bytes(&data));

        // Per PDF spec, the `/DescendantFonts` array
        // must contain exactly one CIDFont reference. This single descendant provides
        // the actual glyph descriptions for the composite font.
        let descendant_fonts_array = dictionary
            .get_or_err("DescendantFonts")?
            .try_array(objects)?;
        if descendant_fonts_array.len() != 1 {
            return Err(FontError::InvalidDescendantFonts(
                "Expected exactly one descendant font",
            ));
        }

        // Retrieve the sole CIDFont dictionary from the array.
        let dictionary = descendant_fonts_array
            .first()
            .ok_or(FontError::InvalidDescendantFonts("Array is empty"))?
            .try_dictionary(objects)?;

        // Determine the CIDFont subtype which dictates how glyph data is stored:
        // - CIDFontType0: Uses CFF (Compact Font Format) glyph descriptions.
        // - CIDFontType2: Uses TrueType glyph descriptions.
        let subtype = match dictionary.get_or_err("Subtype")?.try_str(objects)?.as_ref() {
            "CIDFontType0" => CidFontSubType::Type0,
            "CIDFontType2" => CidFontSubType::Type2,
            other => {
                return Err(FontError::UnsupportedCidFontSubtype {
                    subtype: other.to_string(),
                });
            }
        };

        // The `/DW` (default width) entry specifies the default glyph width in glyph space
        // units (typically 1/1000 of a unit).
        let default_width = dictionary
            .get("DW")
            .map(|dw| dw.try_number::<f32>(objects))
            .transpose()?
            .unwrap_or(Self::DEFAULT_WIDTH);

        // The `/W` array provides individual glyph widths that override the
        // default width for specific CIDs.
        let widths_map = dictionary
            .get("W")
            .map(|obj| -> Result<GlyphWidthsMap, FontError> {
                let resolved_obj = obj.try_array(objects)?;
                GlyphWidthsMap::from_array(resolved_obj, objects).map_err(FontError::from)
            })
            .transpose()?;

        // Process the embedded font data based on the CIDFont subtype:
        // - Type0 (CFF): Rebuild as a standalone CFF font for rendering libraries.
        // - Type2 (TrueType): Use the raw TrueType data directly.
        let font_file = match subtype {
            CidFontSubType::Type0 => Type1Font::read_font_file(dictionary, objects)?,
            CidFontSubType::Type2 => TrueTypeFont::read_font_file(dictionary, objects)?
                .0
                .to_vec(),
        };

        // Build reverse glyph→Unicode map from the embedded font's cmap when
        // ToUnicode is absent and the encoding is Identity-H or Identity-V (i.e.,
        // char code == CID == glyph ID).
        let is_identity_encoding = matches!(
            &encoding,
            Some(FontEncoding::Unknown(s)) if s == "Identity-H" || s == "Identity-V"
        );
        let glyph_to_unicode = if to_unicode.is_none()
            && is_identity_encoding
            && matches!(subtype, CidFontSubType::Type2)
        {
            build_glyph_to_unicode(&font_file)
        } else {
            None
        };

        Ok(Self {
            subtype,
            font_file,
            widths: widths_map,
            encoding,
            default_width,
            to_unicode,
            glyph_to_unicode,
        })
    }
}

/// Build a reverse cmap table: glyph ID → Unicode char.
///
/// Iterates over all (codepoint, glyph_id) pairs in the font's best Unicode
/// cmap subtable and inverts the mapping.  The first Unicode value seen for
/// each glyph ID wins (earlier subtable entries take priority).
fn build_glyph_to_unicode(font_data: &[u8]) -> Option<HashMap<u16, char>> {
    let font = FontRef::new(font_data).ok()?;
    let cmap = font.cmap().ok()?;
    let (_, _, subtable) = cmap.best_subtable()?;

    let mut map = HashMap::new();
    for (codepoint, glyph_id) in subtable.iter() {
        let Some(c) = char::from_u32(codepoint) else {
            continue;
        };
        let gid_u32 = u32::from(glyph_id);
        if let Ok(gid_u16) = u16::try_from(gid_u32) {
            map.entry(gid_u16).or_insert(c);
        }
    }

    if map.is_empty() { None } else { Some(map) }
}
