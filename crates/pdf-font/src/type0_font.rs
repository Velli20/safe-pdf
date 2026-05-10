use std::collections::HashMap;

use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};
use read_fonts::{FontRef, TableProvider};

use crate::{
    error::FontError, glyph_widths_map::GlyphWidthsMap, to_unicode_cmap::ToUnicodeCMap,
    true_type_font::TrueTypeFont, type0_encoding_cmap::Type0EncodingCMap, type1_font::Type1Font,
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
    /// Parsed Type0 encoding CMap that maps text bytes to CIDs.
    pub encoding: Option<Type0EncodingCMap>,
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
            .map(|value| {
                let resolved = objects.resolve_object(value)?;
                match resolved {
                    pdf_object::object_variant::ObjectVariant::Stream(stream) => {
                        Type0EncodingCMap::from_bytes(&stream.data()?)
                    }
                    _ => Type0EncodingCMap::from_name(value.try_str(objects)?.as_ref()),
                }
            })
            .transpose()?;

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
        let is_identity_encoding = encoding
            .as_ref()
            .map(Type0EncodingCMap::is_identity)
            .unwrap_or(false);
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

    /// Decode raw text bytes from a Type0 text-showing operator into CIDs.
    ///
    /// When the font has a parsed `/Encoding` CMap, this uses that CMap's
    /// codespace ranges and CID mappings to consume variable-length character
    /// codes and produce the corresponding CIDs. If a code is unmapped or the
    /// input does not match any codespace range, CID 0 (`.notdef`) is emitted.
    ///
    /// If the font has no parsed encoding CMap, this falls back to the legacy
    /// big-endian 2-byte interpretation used for identity-style composite fonts.
    pub fn decode_bytes_to_cids(&self, text: &[u8]) -> Vec<u16> {
        self.encoding
            .as_ref()
            .map(|encoding| encoding.decode(text))
            .unwrap_or_else(|| {
                let mut decoded = Vec::new();
                let mut chunks = text.chunks_exact(2);
                for pair in &mut chunks {
                    let Some(first) = pair.first().copied() else {
                        continue;
                    };
                    let Some(second) = pair.get(1).copied() else {
                        continue;
                    };
                    decoded.push(u16::from_be_bytes([first, second]));
                }
                if !chunks.remainder().is_empty() {
                    decoded.push(0);
                }
                decoded
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::*;

    fn make_stream_object(
        object_number: usize,
        dictionary: Dictionary,
        data: Vec<u8>,
    ) -> ObjectVariant {
        ObjectVariant::Stream(StreamObject::new(
            object_number,
            0,
            Box::new(dictionary),
            data,
        ))
    }

    #[test]
    fn type0_font_accepts_stream_encoding_and_preserves_tounicode() {
        let encoding_stream = make_stream_object(
            1,
            Dictionary::new(BTreeMap::new()),
            br#"
            begincmap
            /WMode 0 def
            1 begincodespacerange
            <0001> <FFFF>
            endcodespacerange
            1 begincidchar
            <0041> 65
            endcidchar
            endcmap
            "#
            .to_vec(),
        );
        let to_unicode_stream = make_stream_object(
            2,
            Dictionary::new(BTreeMap::new()),
            br#"
            beginbfchar
            <0041> <0042>
            endbfchar
            "#
            .to_vec(),
        );

        let mut file3_dict = BTreeMap::new();
        file3_dict.insert(
            "Subtype".to_string(),
            ObjectVariant::Name(b"OpenType".to_vec()),
        );
        let font_file3 = ObjectVariant::Stream(StreamObject::new(
            3,
            0,
            Box::new(Dictionary::new(file3_dict)),
            vec![0, 1, 2],
        ));

        let mut descriptor_dict = BTreeMap::new();
        descriptor_dict.insert("FontFile3".to_string(), font_file3);

        let mut descendant_dict = BTreeMap::new();
        descendant_dict.insert(
            "Subtype".to_string(),
            ObjectVariant::Name(b"CIDFontType0".to_vec()),
        );
        descendant_dict.insert(
            "FontDescriptor".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(descriptor_dict))),
        );

        let descendant_fonts = vec![ObjectVariant::Dictionary(Box::new(Dictionary::new(
            descendant_dict,
        )))];

        let mut font_dict = BTreeMap::new();
        font_dict.insert(
            "Subtype".to_string(),
            ObjectVariant::Name(b"Type0".to_vec()),
        );
        font_dict.insert("Encoding".to_string(), encoding_stream);
        font_dict.insert("ToUnicode".to_string(), to_unicode_stream);
        font_dict.insert(
            "DescendantFonts".to_string(),
            ObjectVariant::Array(descendant_fonts),
        );

        let font =
            Type0Font::from_dictionary(&Dictionary::new(font_dict), &PassthroughResolver).unwrap();

        assert!(font.encoding.is_some());
        assert_eq!(
            font.to_unicode
                .as_ref()
                .and_then(|cmap| cmap.map_char_code(0x41)),
            Some(['B'].as_slice())
        );
        assert_eq!(font.decode_bytes_to_cids(&[0x00, 0x41]), vec![65]);
    }
}
