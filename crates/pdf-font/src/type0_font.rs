use std::collections::HashMap;

use pdf_cmap::{ToUnicodeCMap, Type0EncodingCMap};
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};
use read_fonts::{FontRef, TableProvider};

use crate::{
    cid_system_info::cid_ordering_from_dictionary,
    error::FontError,
    fallback::fallback_program_from_dictionary,
    font_data::FontData,
    glyph_widths_map::GlyphWidthsMap,
    true_type_font::TrueTypeFont,
    type1_font::{Type1Font, Type1FontProgramFormat},
};

/// Represents a PDF Type0 (composite) font, which references a CIDFont
/// for glyph definitions.
pub struct Type0Font {
    /// The CIDFont subtype (CIDFontType0 or CIDFontType2).
    pub subtype: CidFontSubType,
    /// The actual font program format used for rendering.
    pub program_format: Type0FontProgramFormat,
    /// Font file containing embedded font data.
    pub font_file: FontData,
    /// The embedded Type 1 program format for CIDFontType0 descendants.
    pub type1_program_format: Option<Type1FontProgramFormat>,
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

/// Font program format resolved for a Type0 descendant font.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Type0FontProgramFormat {
    /// OpenType/CFF program rendered through the Type 1/CFF path.
    OpenTypeCff,
    /// TrueType program rendered through the TrueType path.
    TrueType {
        /// Whether decoded CIDs should be mapped through Unicode before glyph lookup.
        cid_to_unicode: bool,
    },
}

impl Type0Font {
    /// Build a Type0 composite font from a PDF font dictionary.
    ///
    /// # Paramaters
    ///
    /// - `dictionary`: The top-level Type0 font dictionary.
    /// - `objects`: The resolver used to dereference indirect PDF objects.
    ///
    /// # Returns
    ///
    /// A parsed [`Type0Font`] with its descendant font, encoding, widths, font
    /// program, and Unicode fallback data resolved.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FontError> {
        let encoding = parse_encoding(dictionary, objects)?;
        let to_unicode = parse_to_unicode(dictionary, objects)?;
        let descendant = Type0DescendantFont::from_dictionary(dictionary, objects)?;
        let Type0FontProgram {
            font_file,
            program_format,
            fallback_cid_to_unicode,
        } = read_type0_font_program(descendant.dictionary, descendant.subtype, objects)?;
        let glyph_to_unicode = glyph_to_unicode_map(
            fallback_cid_to_unicode,
            font_file.as_ref(),
            descendant.subtype,
            encoding.as_ref(),
            to_unicode.as_ref(),
        );

        Ok(Self {
            subtype: descendant.subtype,
            program_format,
            font_file,
            type1_program_format: if matches!(descendant.subtype, CidFontSubType::Type0) {
                Some(Type1FontProgramFormat::OpenTypeCff)
            } else {
                None
            },
            widths: descendant.widths,
            encoding,
            default_width: descendant.default_width,
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
    ///
    /// # Paramaters
    ///
    /// - `text`: The raw bytes from a Type0 text-showing operator.
    ///
    /// # Returns
    ///
    /// The decoded CID sequence. Malformed trailing identity bytes decode to
    /// CID 0 as a best-effort `.notdef` replacement.
    pub fn decode_bytes_to_cids(&self, text: &[u8]) -> Vec<u16> {
        self.encoding
            .as_ref()
            .map(|encoding| encoding.decode(text))
            .unwrap_or_else(|| Type0EncodingCMap::decode_identity(text))
    }
}

struct Type0DescendantFont<'a> {
    dictionary: &'a Dictionary,
    subtype: CidFontSubType,
    default_width: f32,
    widths: Option<GlyphWidthsMap>,
}

impl<'a> Type0DescendantFont<'a> {
    /// Parse the single descendant CIDFont referenced by a Type0 font.
    ///
    /// # Paramaters
    ///
    /// - `dictionary`: The top-level Type0 font dictionary.
    /// - `objects`: The resolver used to dereference indirect PDF objects.
    ///
    /// # Returns
    ///
    /// The resolved descendant font dictionary together with its subtype,
    /// default width, and optional width overrides.
    fn from_dictionary(
        dictionary: &'a Dictionary,
        objects: &'a dyn ObjectResolver,
    ) -> Result<Self, FontError> {
        let dictionary = descendant_font_dictionary(dictionary, objects)?;

        Ok(Self {
            dictionary,
            subtype: cid_font_subtype(dictionary, objects)?,
            default_width: default_width(dictionary, objects)?,
            widths: widths_map(dictionary, objects)?,
        })
    }
}

struct Type0FontProgram {
    font_file: FontData,
    program_format: Type0FontProgramFormat,
    fallback_cid_to_unicode: Option<HashMap<u16, char>>,
}

/// Parse the optional `/Encoding` entry of a Type0 font dictionary.
///
/// # Paramaters
///
/// - `dictionary`: The top-level Type0 font dictionary.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// The parsed Type0 encoding CMap when the dictionary contains `/Encoding`.
fn parse_encoding(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Option<Type0EncodingCMap>, FontError> {
    dictionary
        .get("Encoding")
        .map(|value| {
            let resolved = objects.resolve_object(value)?;
            match resolved {
                ObjectVariant::Stream(stream) => {
                    Ok(Type0EncodingCMap::from_bytes(stream.raw_data())?)
                }
                _ => Ok(Type0EncodingCMap::from_name(value.try_str(objects)?)?),
            }
        })
        .transpose()
}

/// Parse the optional `/ToUnicode` CMap from a Type0 font dictionary.
///
/// # Paramaters
///
/// - `dictionary`: The top-level Type0 font dictionary.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// The parsed ToUnicode CMap when a valid `/ToUnicode` stream is present.
fn parse_to_unicode(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Option<ToUnicodeCMap>, FontError> {
    dictionary
        .get("ToUnicode")
        .and_then(|e| e.try_stream(objects).ok())
        .map(|s| ToUnicodeCMap::try_from(s.raw_data()))
        .transpose()
        .map_err(FontError::from)
}

/// Resolve the sole descendant CIDFont dictionary from `/DescendantFonts`.
///
/// # Paramaters
///
/// - `dictionary`: The top-level Type0 font dictionary.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// The resolved descendant dictionary, or an invalid-descendant error when the
/// array does not contain exactly one entry.
fn descendant_font_dictionary<'a>(
    dictionary: &'a Dictionary,
    objects: &'a dyn ObjectResolver,
) -> Result<&'a Dictionary, FontError> {
    let descendant_fonts = dictionary.required_array("DescendantFonts", objects)?;
    if descendant_fonts.len() != 1 {
        return Err(FontError::InvalidDescendantFonts(
            "Expected exactly one descendant font",
        ));
    }

    descendant_fonts
        .first()
        .ok_or(FontError::InvalidDescendantFonts("Array is empty"))?
        .try_dictionary(objects)
        .map_err(FontError::from)
}

/// Parse the CIDFont subtype from a descendant font dictionary.
///
/// # Paramaters
///
/// - `dictionary`: The descendant CIDFont dictionary.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// The supported CIDFont subtype, or an unsupported-subtype error for unknown
/// subtype names.
fn cid_font_subtype(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<CidFontSubType, FontError> {
    match dictionary.required_str("Subtype", objects)? {
        "CIDFontType0" => Ok(CidFontSubType::Type0),
        "CIDFontType2" => Ok(CidFontSubType::Type2),
        other => Err(FontError::UnsupportedCidFontSubtype {
            subtype: other.to_string(),
        }),
    }
}

/// Parse the default glyph width from a descendant font dictionary.
///
/// # Paramaters
///
/// - `dictionary`: The descendant CIDFont dictionary.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// The `/DW` value, or [`Type0Font::DEFAULT_WIDTH`] when `/DW` is absent.
fn default_width(dictionary: &Dictionary, objects: &dyn ObjectResolver) -> Result<f32, FontError> {
    Ok(dictionary
        .get("DW")
        .map(|dw| dw.try_number::<f32>(objects))
        .transpose()?
        .unwrap_or(Type0Font::DEFAULT_WIDTH))
}

/// Parse explicit CID width overrides from a descendant font dictionary.
///
/// # Paramaters
///
/// - `dictionary`: The descendant CIDFont dictionary.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// The parsed `/W` width map when the dictionary contains width overrides.
fn widths_map(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Option<GlyphWidthsMap>, FontError> {
    dictionary
        .get("W")
        .map(|obj| {
            let widths = obj.try_array(objects)?;
            GlyphWidthsMap::from_array(widths, objects).map_err(FontError::from)
        })
        .transpose()
}

/// Read or synthesize the font program for a Type0 descendant font.
///
/// # Paramaters
///
/// - `dictionary`: The descendant CIDFont dictionary.
/// - `subtype`: The parsed CIDFont subtype.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// The resolved font bytes, rendering program format, and any synthetic
/// CID-to-Unicode fallback map.
fn read_type0_font_program(
    dictionary: &Dictionary,
    subtype: CidFontSubType,
    objects: &dyn ObjectResolver,
) -> Result<Type0FontProgram, FontError> {
    match subtype {
        CidFontSubType::Type0 => read_cid_font_type0_program(dictionary, objects),
        CidFontSubType::Type2 => Ok(Type0FontProgram {
            font_file: TrueTypeFont::read_font_file(dictionary, objects)?.0,
            program_format: Type0FontProgramFormat::TrueType {
                cid_to_unicode: false,
            },
            fallback_cid_to_unicode: None,
        }),
    }
}

/// Read the font program for a CIDFontType0 descendant.
///
/// # Paramaters
///
/// - `dictionary`: The descendant CIDFont dictionary.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// An OpenType/CFF program when embedded, or a synthesized TrueType fallback
/// program when the embedded font file is missing.
fn read_cid_font_type0_program(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Type0FontProgram, FontError> {
    match Type1Font::read_font_file(dictionary, objects) {
        Ok((font_file, Type1FontProgramFormat::OpenTypeCff)) => Ok(Type0FontProgram {
            font_file,
            program_format: Type0FontProgramFormat::OpenTypeCff,
            fallback_cid_to_unicode: None,
        }),
        Ok((_, Type1FontProgramFormat::ClassicType1)) => Err(FontError::UnsupportedFontSubtype {
            subtype: "FontFile".to_string(),
        }),
        Err(FontError::MissingFontFile) => fallback_type0_program(dictionary, objects),
        Err(err) => Err(err),
    }
}

/// Build the fallback program used when a CIDFontType0 has no embedded font.
///
/// # Paramaters
///
/// - `dictionary`: The descendant CIDFont dictionary.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// A synthetic TrueType-backed Type0 program plus an optional CID-to-Unicode
/// map derived from the CIDSystemInfo ordering.
fn fallback_type0_program(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Type0FontProgram, FontError> {
    let fallback = fallback_program_from_dictionary(dictionary, objects)?;
    let fallback_cid_to_unicode = cid_to_unicode_map(dictionary, objects)?;

    Ok(Type0FontProgram {
        font_file: fallback.font_file.into(),
        program_format: Type0FontProgramFormat::TrueType {
            cid_to_unicode: fallback_cid_to_unicode.is_some(),
        },
        fallback_cid_to_unicode,
    })
}

/// Select the Unicode fallback map for decoded Type0 CIDs.
///
/// # Paramaters
///
/// - `fallback_cid_to_unicode`: A synthetic fallback map from CJK CID ordering.
/// - `font_file`: The resolved embedded or fallback font bytes.
/// - `subtype`: The parsed CIDFont subtype.
/// - `encoding`: The parsed Type0 encoding CMap, when present.
/// - `to_unicode`: The parsed ToUnicode CMap, when present.
///
/// # Returns
///
/// A CID/glyph-to-Unicode map when one can be built without overriding an
/// explicit ToUnicode CMap.
fn glyph_to_unicode_map(
    fallback_cid_to_unicode: Option<HashMap<u16, char>>,
    font_file: &[u8],
    subtype: CidFontSubType,
    encoding: Option<&Type0EncodingCMap>,
    to_unicode: Option<&ToUnicodeCMap>,
) -> Option<HashMap<u16, char>> {
    if fallback_cid_to_unicode.is_some() {
        return fallback_cid_to_unicode;
    }

    let is_identity_encoding = encoding
        .map(Type0EncodingCMap::is_identity)
        .unwrap_or(false);
    if to_unicode.is_none() && is_identity_encoding && matches!(subtype, CidFontSubType::Type2) {
        build_glyph_to_unicode(font_file)
    } else {
        None
    }
}

/// Build a reverse cmap table: glyph ID → Unicode char.
///
/// Iterates over all (codepoint, glyph_id) pairs in the font's best Unicode
/// cmap subtable and inverts the mapping.  The first Unicode value seen for
/// each glyph ID wins (earlier subtable entries take priority).
///
/// # Paramaters
///
/// - `font_data`: Raw TrueType/OpenType font bytes.
///
/// # Returns
///
/// A glyph ID to Unicode map when the font contains a readable cmap table with
/// at least one valid Unicode scalar value.
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

/// Build a fallback CID-to-Unicode map from a descendant font's CID ordering.
///
/// # Paramaters
///
/// - `descendant_font`: The descendant CIDFont dictionary.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// A CID-to-Unicode map for known CJK orderings, or `None` when the ordering is
/// absent or unsupported.
fn cid_to_unicode_map(
    descendant_font: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Option<HashMap<u16, char>>, FontError> {
    let Some(ordering) = cid_ordering_from_dictionary(descendant_font, objects)? else {
        return Ok(None);
    };

    Ok(ordering.cid_to_unicode_map()?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };
    use read_fonts::TableProvider;

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
        let font_file3_stream =
            StreamObject::new(3, 0, Box::new(Dictionary::new(file3_dict)), vec![0, 1, 2]);
        let font_file3_bytes = font_file3_stream.raw_data().as_ptr();
        let font_file3 = ObjectVariant::Stream(font_file3_stream);

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
        descendant_dict.insert(
            "CIDSystemInfo".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                "Ordering".to_string(),
                ObjectVariant::LiteralString(b"Japan1".to_vec()),
            )])))),
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

        assert_eq!(font.program_format, Type0FontProgramFormat::OpenTypeCff);
        assert!(matches!(&font.font_file, FontData::Shared(_)));
        assert_eq!(font.font_file.as_ptr(), font_file3_bytes);
        assert!(font.encoding.is_some());
        assert_eq!(
            font.to_unicode
                .as_ref()
                .and_then(|cmap| cmap.map_char_code(0x41)),
            Some(['B'].as_slice())
        );
        assert_eq!(font.decode_bytes_to_cids(&[0x00, 0x41]), vec![65]);
    }

    #[test]
    fn type0_font_accepts_bfchar_encoding_streams() {
        let encoding_stream = make_stream_object(
            1,
            Dictionary::new(BTreeMap::new()),
            br#"
            begincmap
            /WMode 0 def
            1 begincodespacerange
            <0000> <FFFF>
            endcodespacerange
            1 beginbfchar
            <0043> <0046>
            endbfchar
            endcmap
            %%EOF
            "#
            .to_vec(),
        );

        let mut descriptor_dict = BTreeMap::new();
        descriptor_dict.insert(
            "FontFile2".to_string(),
            ObjectVariant::Stream(StreamObject::new(
                3,
                0,
                Box::new(Dictionary::new(BTreeMap::new())),
                vec![0, 1, 0, 0],
            )),
        );

        let mut descendant_dict = BTreeMap::new();
        descendant_dict.insert(
            "Subtype".to_string(),
            ObjectVariant::Name(b"CIDFontType2".to_vec()),
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
        font_dict.insert(
            "DescendantFonts".to_string(),
            ObjectVariant::Array(descendant_fonts),
        );

        let font =
            Type0Font::from_dictionary(&Dictionary::new(font_dict), &PassthroughResolver).unwrap();

        assert_eq!(font.decode_bytes_to_cids(&[0x00, 0x43]), vec![70]);
    }

    #[test]
    fn cid_font_type0_missing_font_file_uses_truetype_fallback() {
        let mut descriptor_dict = BTreeMap::new();
        descriptor_dict.insert("Flags".to_string(), ObjectVariant::Integer(0));

        let mut descendant_dict = BTreeMap::new();
        descendant_dict.insert(
            "Subtype".to_string(),
            ObjectVariant::Name(b"CIDFontType0".to_vec()),
        );
        descendant_dict.insert(
            "BaseFont".to_string(),
            ObjectVariant::Name(b"Ryumin-Light".to_vec()),
        );
        descendant_dict.insert(
            "FontDescriptor".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(descriptor_dict))),
        );
        descendant_dict.insert(
            "CIDSystemInfo".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                "Ordering".to_string(),
                ObjectVariant::LiteralString(b"Japan1".to_vec()),
            )])))),
        );

        let mut font_dict = BTreeMap::new();
        font_dict.insert(
            "Subtype".to_string(),
            ObjectVariant::Name(b"Type0".to_vec()),
        );
        font_dict.insert(
            "BaseFont".to_string(),
            ObjectVariant::Name(b"Ryumin-Light-90ms-RKSJ-H".to_vec()),
        );
        font_dict.insert(
            "DescendantFonts".to_string(),
            ObjectVariant::Array(vec![ObjectVariant::Dictionary(Box::new(Dictionary::new(
                descendant_dict,
            )))]),
        );

        let font =
            Type0Font::from_dictionary(&Dictionary::new(font_dict), &PassthroughResolver).unwrap();

        assert_eq!(font.subtype, CidFontSubType::Type0);
        assert_eq!(
            font.program_format,
            Type0FontProgramFormat::TrueType {
                cid_to_unicode: true
            }
        );
        assert!(!font.font_file.is_empty());
        assert_eq!(
            font.decode_bytes_to_cids(&[0x00, 0x41, 0x12, 0x34, 0xFF]),
            vec![65, 0x1234, 0]
        );
    }

    #[test]
    fn cid_font_type0_cjk_fallback_decodes_issue_13343_text() {
        let font = issue_13343_font();

        assert_eq!(
            issue_13343_text_to_unicode(
                &font,
                &[
                    0x28, 0x35, 0x37, 0x29, 0x81, 0x79, 0x97, 0x76, 0x96, 0xF1, 0x81, 0x7A,
                ],
            ),
            "(57)\u{3010}\u{8981}\u{7d04}\u{3011}"
        );
        assert_eq!(
            issue_13343_text_to_unicode(
                &font,
                &[
                    0x28, 0x38, 0x31, 0x29, 0x8E, 0x77, 0x92, 0xE8, 0x8D, 0x91, 0x81, 0x45, 0x92,
                    0x6E, 0x88, 0xE6, 0x81, 0x40, 0x20, 0x20, 0x41, 0x50,
                ],
            ),
            "(81)\u{6307}\u{5b9a}\u{56fd}\u{30fb}\u{5730}\u{57df}\u{2003}  AP"
        );
    }

    #[test]
    fn cid_font_type0_cjk_fallback_font_covers_issue_13343_glyphs() {
        let font = issue_13343_font();
        let font_ref = FontRef::new(&font.font_file).unwrap();
        let cmap = font_ref.cmap().unwrap();
        let (_, _, subtable) = cmap.best_subtable().unwrap();

        for c in [
            '\u{3010}', '\u{8981}', '\u{7d04}', '\u{3011}', '\u{6307}', '\u{5b9a}', '\u{56fd}',
            '\u{5730}', '\u{57df}', '\u{2003}',
        ] {
            assert!(
                subtable.map_codepoint(u32::from(c)).is_some(),
                "missing fallback glyph for U+{:04X}",
                u32::from(c)
            );
        }
    }

    fn issue_13343_font() -> Type0Font {
        let mut descriptor_dict = BTreeMap::new();
        descriptor_dict.insert("Flags".to_string(), ObjectVariant::Integer(6));

        let mut descendant_dict = BTreeMap::new();
        descendant_dict.insert(
            "Subtype".to_string(),
            ObjectVariant::Name(b"CIDFontType0".to_vec()),
        );
        descendant_dict.insert(
            "BaseFont".to_string(),
            ObjectVariant::Name(b"Ryumin-Light".to_vec()),
        );
        descendant_dict.insert(
            "FontDescriptor".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(descriptor_dict))),
        );
        descendant_dict.insert(
            "CIDSystemInfo".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                "Ordering".to_string(),
                ObjectVariant::LiteralString(b"Japan1".to_vec()),
            )])))),
        );

        let mut font_dict = BTreeMap::new();
        font_dict.insert(
            "Subtype".to_string(),
            ObjectVariant::Name(b"Type0".to_vec()),
        );
        font_dict.insert(
            "BaseFont".to_string(),
            ObjectVariant::Name(b"Ryumin-Light-90ms-RKSJ-H".to_vec()),
        );
        font_dict.insert(
            "Encoding".to_string(),
            ObjectVariant::Name(b"90ms-RKSJ-H".to_vec()),
        );
        font_dict.insert(
            "DescendantFonts".to_string(),
            ObjectVariant::Array(vec![ObjectVariant::Dictionary(Box::new(Dictionary::new(
                descendant_dict,
            )))]),
        );

        Type0Font::from_dictionary(&Dictionary::new(font_dict), &PassthroughResolver).unwrap()
    }

    fn issue_13343_text_to_unicode(font: &Type0Font, text: &[u8]) -> String {
        font.decode_bytes_to_cids(text)
            .iter()
            .filter_map(|cid| font.glyph_to_unicode.as_ref().and_then(|map| map.get(cid)))
            .collect()
    }
}
