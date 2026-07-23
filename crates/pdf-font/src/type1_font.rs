use std::{collections::HashMap, sync::Arc};

use pdf_cmap::ToUnicodeCMap;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

use crate::{
    cff_builder::build_cff_font,
    encoding::{Encoding, FontEncoding},
    error::FontError,
    font_data::FontData,
    simple_font_glyph_map::SimpleFontGlyphWidthsMap,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type1FontProgramFormat {
    OpenTypeCff,
    ClassicType1,
}

/// Minimal, initial representation of a PDF Type1 font.
///
/// This focuses on dictionary-level metadata needed by higher layers
/// and defers actual glyph rendering or embedded program parsing.
pub struct Type1Font {
    /// A stream containing the font program.
    pub font_file: FontData,
    /// The format of `font_file`.
    pub program_format: Type1FontProgramFormat,
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
        let (font_file, program_format) = Self::read_font_file(dictionary, objects)?;

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
            .map(|s| ToUnicodeCMap::try_from(s.raw_data()))
            .transpose()?;

        Ok(Self {
            font_file,
            program_format,
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
    ///    Returned as classic Type 1 bytes after trimming with `/Length1..3`
    ///    when available and validating the program with `read-fonts`.
    ///
    /// If neither stream is present (or there is no `/FontDescriptor` at all),
    /// returns [`FontError::MissingFontFile`].  The caller (`Font::from_dictionary`)
    /// catches that sentinel and falls back to a bundled Standard 14 substitute.
    ///
    /// # Errors
    ///
    /// - [`FontError::MissingFontFile`] — no embedded font program found.
    /// - [`FontError::UnsupportedFontSubtype`] — unsupported `FontFile3` subtype
    ///   or an invalid `FontFile` (classic Type 1) stream.
    /// - Any [`FontError`] propagated from stream decompression or CFF building.
    pub(crate) fn read_font_file(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<(FontData, Type1FontProgramFormat), FontError> {
        let descriptor = dictionary.required_dictionary("FontDescriptor", objects)?;

        // Path 1: FontFile3 stream with subtype-driven handling.
        if let Some(font_file3) = descriptor.get("FontFile3") {
            let stream = font_file3.try_stream(objects)?;
            let subtype = stream.dictionary.optional_str("Subtype", objects)?;

            return match subtype {
                Some("Type1C") | Some("CIDFontType0C") => build_cff_font(stream.raw_data())
                    .map(|font| (font.into(), Type1FontProgramFormat::OpenTypeCff)),
                Some("OpenType") => Ok((
                    FontData::shared(stream.shared_data()),
                    Type1FontProgramFormat::OpenTypeCff,
                )),
                Some(other) => Err(FontError::UnsupportedFontSubtype {
                    subtype: other.to_string(),
                }),
                None => Err(FontError::UnsupportedFontSubtype {
                    subtype: "FontFile3".to_string(),
                }),
            };
        }

        // Path 2: classic Type 1 in FontFile.
        if let Some(font_file) = descriptor.get("FontFile") {
            let stream = font_file.try_stream(objects)?;
            let normalized =
                normalize_classic_type1_bytes(descriptor, stream.shared_data(), objects)?;
            read_fonts::ps::type1::Type1Font::new(normalized.as_ref()).map_err(|_| {
                FontError::UnsupportedFontSubtype {
                    subtype: "FontFile".to_string(),
                }
            })?;
            return Ok((normalized, Type1FontProgramFormat::ClassicType1));
        }

        // No embedded font program at all.
        Err(FontError::MissingFontFile)
    }
}

fn normalize_classic_type1_bytes(
    descriptor: &Dictionary,
    data: Arc<Vec<u8>>,
    objects: &dyn ObjectResolver,
) -> Result<FontData, FontError> {
    if is_pfb(data.as_slice()) {
        return Ok(FontData::shared(data));
    }

    let visible_len = classic_type1_length(descriptor, data.len(), objects)?.unwrap_or(data.len());
    Ok(FontData::shared_prefix(data, visible_len))
}

fn classic_type1_length(
    descriptor: &Dictionary,
    data_len: usize,
    objects: &dyn ObjectResolver,
) -> Result<Option<usize>, FontError> {
    let length1 = descriptor.optional_number::<usize>("Length1", objects)?;
    let length2 = descriptor.optional_number::<usize>("Length2", objects)?;
    let length3 = descriptor.optional_number::<usize>("Length3", objects)?;

    let Some(total_length) = length1
        .zip(length2)
        .and_then(|(length1, length2)| length3.map(|length3| (length1, length2, length3)))
        .and_then(|(length1, length2, length3)| {
            length1
                .checked_add(length2)
                .and_then(|sum| sum.checked_add(length3))
        })
    else {
        return Ok(None);
    };

    Ok(Some(total_length.min(data_len)))
}

fn is_pfb(data: &[u8]) -> bool {
    data.len() >= 2 && data.first() == Some(&0x80) && matches!(data.get(1), Some(0x01 | 0x02))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{
        object_resolver::PassthroughResolver, object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::*;

    const EEXEC_SEED: u16 = 55665;

    fn make_font_dict_with_font_file3(subtype: Option<&str>, bytes: Vec<u8>) -> Dictionary {
        let mut file3_stream_dict = BTreeMap::new();
        if let Some(subtype) = subtype {
            file3_stream_dict.insert(
                "Subtype".to_string(),
                ObjectVariant::Name(subtype.as_bytes().to_vec()),
            );
        }
        let font_file3_stream =
            StreamObject::new(10, 0, Box::new(Dictionary::new(file3_stream_dict)), bytes);

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

    fn make_font_dict_with_font_file(
        bytes: Vec<u8>,
        lengths: Option<(usize, usize, usize)>,
    ) -> Dictionary {
        let mut descriptor_dict = BTreeMap::new();
        if let Some((length1, length2, length3)) = lengths {
            descriptor_dict.insert(
                "Length1".to_string(),
                ObjectVariant::Integer(length1 as i64),
            );
            descriptor_dict.insert(
                "Length2".to_string(),
                ObjectVariant::Integer(length2 as i64),
            );
            descriptor_dict.insert(
                "Length3".to_string(),
                ObjectVariant::Integer(length3 as i64),
            );
        }
        descriptor_dict.insert(
            "FontFile".to_string(),
            ObjectVariant::Stream(StreamObject::new(
                11,
                0,
                Box::new(Dictionary::new(BTreeMap::new())),
                bytes,
            )),
        );

        let mut font_dict = BTreeMap::new();
        font_dict.insert(
            "FontDescriptor".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(descriptor_dict))),
        );
        Dictionary::new(font_dict)
    }

    fn encrypt(bytes: &[u8], seed: u16) -> Vec<u8> {
        let mut r = seed;
        let mut out = Vec::with_capacity(bytes.len());
        for &plain in bytes {
            let cipher = plain ^ ((r >> 8) as u8);
            out.push(cipher);
            r = u16::try_from(
                (u32::from(cipher) + u32::from(r))
                    .wrapping_mul(52845)
                    .wrapping_add(22719)
                    & 0xFFFF,
            )
            .unwrap();
        }
        out
    }

    fn minimal_type1_segments() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let cleartext = br#"%!FontType1-1.0: DummyFont 1.0
10 dict begin
/FontName /DummyFont def
/FontType 1 def
/FontMatrix [0.001 0 0 0.001 0 0] readonly def
/FontBBox [0 0 0 0] readonly def
/Encoding StandardEncoding def
currentdict end
currentfile eexec
"#
        .to_vec();

        let private_plain = b"/Private 1 dict dup begin\n/lenIV -1 def\n/CharStrings 1 dict dup begin\n/.notdef 1 RD \x0E ND\nend\nend\nmark currentfile closefile\n";
        let mut encrypted_private = vec![0, 0, 0, 0];
        encrypted_private.extend_from_slice(private_plain);
        let encrypted_private = encrypt(&encrypted_private, EEXEC_SEED);

        let trailer = b"0000000000000000000000000000000000000000\ncleartomark\n".to_vec();
        (cleartext, encrypted_private, trailer)
    }

    fn minimal_pfa_font() -> (Vec<u8>, (usize, usize, usize)) {
        let (cleartext, encrypted_private, trailer) = minimal_type1_segments();
        let lengths = (cleartext.len(), encrypted_private.len(), trailer.len());
        let mut bytes = cleartext;
        bytes.extend_from_slice(&encrypted_private);
        bytes.extend_from_slice(&trailer);
        (bytes, lengths)
    }

    fn minimal_pfb_font() -> Vec<u8> {
        let (cleartext, encrypted_private, trailer) = minimal_type1_segments();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x80, 0x01]);
        bytes.extend_from_slice(&(cleartext.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&cleartext);
        bytes.extend_from_slice(&[0x80, 0x02]);
        bytes.extend_from_slice(&((encrypted_private.len() + trailer.len()) as u32).to_le_bytes());
        bytes.extend_from_slice(&encrypted_private);
        bytes.extend_from_slice(&trailer);
        bytes.extend_from_slice(&[0x80, 0x03, 0x00, 0x00, 0x00, 0x00]);
        bytes
    }

    #[test]
    fn font_file3_type1c_is_wrapped_into_opentype() {
        let cff_bytes = vec![1, 2, 3, 4, 5];
        let dict = make_font_dict_with_font_file3(Some("Type1C"), cff_bytes.clone());

        let (parsed, format) = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap();
        let expected = build_cff_font(&cff_bytes).unwrap();
        assert_eq!(format, Type1FontProgramFormat::OpenTypeCff);
        assert_eq!(parsed.as_ref(), expected.as_slice());
        assert!(matches!(parsed, FontData::Owned(_)));
    }

    #[test]
    fn font_file3_opentype_is_returned_as_is() {
        let opentype_bytes = vec![0, 1, 2, 3, 4, 5];
        let dict = make_font_dict_with_font_file3(Some("OpenType"), opentype_bytes.clone());
        let stream_bytes = dict
            .required_dictionary("FontDescriptor", &PassthroughResolver)
            .unwrap()
            .required_stream("FontFile3", &PassthroughResolver)
            .unwrap()
            .raw_data()
            .as_ptr();

        let (parsed, format) = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap();
        assert_eq!(format, Type1FontProgramFormat::OpenTypeCff);
        assert_eq!(parsed.as_ref(), opentype_bytes.as_slice());
        assert_eq!(parsed.as_ptr(), stream_bytes);
        assert!(matches!(parsed, FontData::Shared(_)));
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

    #[test]
    fn font_file_pfa_is_returned_as_classic_type1() {
        let (bytes, lengths) = minimal_pfa_font();
        let dict = make_font_dict_with_font_file(bytes.clone(), Some(lengths));
        let stream_bytes = dict
            .required_dictionary("FontDescriptor", &PassthroughResolver)
            .unwrap()
            .required_stream("FontFile", &PassthroughResolver)
            .unwrap()
            .raw_data()
            .as_ptr();

        let (parsed, format) = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap();
        assert_eq!(format, Type1FontProgramFormat::ClassicType1);
        assert_eq!(parsed.as_ref(), bytes.as_slice());
        assert_eq!(parsed.as_ptr(), stream_bytes);
    }

    #[test]
    fn font_file_pfb_is_returned_as_classic_type1() {
        let bytes = minimal_pfb_font();
        let dict = make_font_dict_with_font_file(bytes.clone(), None);
        let stream_bytes = dict
            .required_dictionary("FontDescriptor", &PassthroughResolver)
            .unwrap()
            .required_stream("FontFile", &PassthroughResolver)
            .unwrap()
            .raw_data()
            .as_ptr();

        let (parsed, format) = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap();
        assert_eq!(format, Type1FontProgramFormat::ClassicType1);
        assert_eq!(parsed.as_ref(), bytes.as_slice());
        assert_eq!(parsed.as_ptr(), stream_bytes);
    }

    #[test]
    fn font_file_lengths_trim_trailing_padding() {
        let (mut bytes, lengths) = minimal_pfa_font();
        bytes.extend_from_slice(b"trailing junk");
        let dict = make_font_dict_with_font_file(bytes, Some(lengths));
        let stream_bytes = dict
            .required_dictionary("FontDescriptor", &PassthroughResolver)
            .unwrap()
            .required_stream("FontFile", &PassthroughResolver)
            .unwrap()
            .raw_data()
            .as_ptr();

        let (parsed, format) = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap();
        assert_eq!(format, Type1FontProgramFormat::ClassicType1);
        let expected_len = lengths.0 + lengths.1 + lengths.2;
        assert_eq!(parsed.len(), expected_len);
        assert_eq!(parsed.as_ptr(), stream_bytes);
    }

    #[test]
    fn font_file_invalid_data_is_unsupported() {
        let dict = make_font_dict_with_font_file(b"not a type1 font".to_vec(), None);

        let err = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap_err();
        assert_eq!(
            err,
            FontError::UnsupportedFontSubtype {
                subtype: "FontFile".to_string()
            }
        );
    }
}
