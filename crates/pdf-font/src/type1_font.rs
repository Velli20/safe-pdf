use std::{collections::HashMap, sync::Arc};

use pdf_cmap::ToUnicodeCMap;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{
    cff_builder::build_cff_font, encoding::Encoding, error::FontError, font_data::FontData,
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

        let encoding = Encoding::from_dictionary(dictionary, objects)?.unwrap_or_default();

        let to_unicode = ToUnicodeCMap::from_dictionary(dictionary, objects)?;

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
        let descriptor = dictionary.required_dictionary(b"FontDescriptor", objects)?;

        // Path 1: FontFile3 stream with subtype-driven handling.
        if let Some(font_file3) = descriptor.get(b"FontFile3") {
            let stream = font_file3.try_stream(objects)?;
            let subtype = stream.dictionary.optional_bytes(b"Subtype", objects)?;

            return match subtype {
                Some(b"Type1C") | Some(b"CIDFontType0C") => {
                    let cff_data = stream.raw_data();
                    if cff_data.is_empty() {
                        return Err(FontError::MissingFontFile);
                    }
                    build_cff_font(cff_data)
                        .map(|font| (font.into(), Type1FontProgramFormat::OpenTypeCff))
                }
                Some(b"OpenType") => Ok((
                    FontData::shared(stream.shared_data()),
                    Type1FontProgramFormat::OpenTypeCff,
                )),
                Some(other) => Err(FontError::UnsupportedFontSubtype {
                    subtype: String::from_utf8_lossy(other).into_owned(),
                }),
                None => Err(FontError::UnsupportedFontSubtype {
                    subtype: "FontFile3".to_string(),
                }),
            };
        }

        // Path 2: classic Type 1 in FontFile.
        if let Some(font_file) = descriptor.get(b"FontFile") {
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
    let length1 = descriptor.optional_number::<usize>(b"Length1", objects)?;
    let length2 = descriptor.optional_number::<usize>(b"Length2", objects)?;
    let length3 = descriptor.optional_number::<usize>(b"Length3", objects)?;

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

    use pdf_content_stream::ContentStreamIdAllocator;
    use pdf_object::{
        object_resolver::PassthroughResolver, object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::*;
    use crate::{font::Font, standard14::Standard14Font};

    const EEXEC_SEED: u16 = 55665;

    fn make_font_dict_with_font_file3(subtype: Option<&str>, bytes: Vec<u8>) -> Dictionary {
        let mut file3_stream_dict = BTreeMap::new();
        if let Some(subtype) = subtype {
            file3_stream_dict.insert(
                Vec::from(b"Subtype"),
                ObjectVariant::Name(subtype.as_bytes().to_vec()),
            );
        }
        let font_file3_stream =
            StreamObject::new(10, 0, Box::new(Dictionary::new(file3_stream_dict)), bytes);

        let mut descriptor_dict = BTreeMap::new();
        descriptor_dict.insert(
            Vec::from(b"FontFile3"),
            ObjectVariant::Stream(font_file3_stream),
        );

        let mut font_dict = BTreeMap::new();
        font_dict.insert(
            Vec::from(b"FontDescriptor"),
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
                Vec::from(b"Length1"),
                ObjectVariant::Integer(length1 as i64),
            );
            descriptor_dict.insert(
                Vec::from(b"Length2"),
                ObjectVariant::Integer(length2 as i64),
            );
            descriptor_dict.insert(
                Vec::from(b"Length3"),
                ObjectVariant::Integer(length3 as i64),
            );
        }
        descriptor_dict.insert(
            Vec::from(b"FontFile"),
            ObjectVariant::Stream(StreamObject::new(
                11,
                0,
                Box::new(Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new())),
                bytes,
            )),
        );

        let mut font_dict = BTreeMap::new();
        font_dict.insert(
            Vec::from(b"FontDescriptor"),
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
        assert!(matches!(parsed, FontData::Shared(_)));
    }

    #[test]
    fn empty_compact_font_file_is_treated_as_missing() {
        for subtype in ["Type1C", "CIDFontType0C"] {
            let dict = make_font_dict_with_font_file3(Some(subtype), Vec::new());

            let err = Type1Font::read_font_file(&dict, &PassthroughResolver).unwrap_err();
            assert_eq!(err, FontError::MissingFontFile);
        }
    }

    #[test]
    fn empty_type1c_uses_standard14_fallback() {
        let mut dict = make_font_dict_with_font_file3(Some("Type1C"), Vec::new());
        dict.dictionary
            .insert(b"Subtype".to_vec(), ObjectVariant::Name(b"Type1".to_vec()));
        dict.dictionary.insert(
            b"BaseFont".to_vec(),
            ObjectVariant::Name(b"Helvetica".to_vec()),
        );
        let mut id_allocator = ContentStreamIdAllocator::new();

        let font = Font::from_dictionary(&dict, &PassthroughResolver, &mut id_allocator);

        let Font::TrueType(font) = font else {
            panic!("empty Type1C font should use a TrueType fallback");
        };
        assert_eq!(font.standard14, Some(Standard14Font::Helvetica));
    }

    #[test]
    fn font_file3_opentype_is_returned_as_is() {
        let opentype_bytes = vec![0, 1, 2, 3, 4, 5];
        let dict = make_font_dict_with_font_file3(Some("OpenType"), opentype_bytes.clone());
        let stream_bytes = dict
            .required_dictionary(b"FontDescriptor", &PassthroughResolver)
            .unwrap()
            .required_stream(b"FontFile3", &PassthroughResolver)
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
                subtype: "Type42".to_owned()
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
                subtype: "FontFile3".to_owned()
            }
        );
    }

    #[test]
    fn font_file_pfa_is_returned_as_classic_type1() {
        let (bytes, lengths) = minimal_pfa_font();
        let dict = make_font_dict_with_font_file(bytes.clone(), Some(lengths));
        let stream_bytes = dict
            .required_dictionary(b"FontDescriptor", &PassthroughResolver)
            .unwrap()
            .required_stream(b"FontFile", &PassthroughResolver)
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
            .required_dictionary(b"FontDescriptor", &PassthroughResolver)
            .unwrap()
            .required_stream(b"FontFile", &PassthroughResolver)
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
            .required_dictionary(b"FontDescriptor", &PassthroughResolver)
            .unwrap()
            .required_stream(b"FontFile", &PassthroughResolver)
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
                subtype: "FontFile".to_owned()
            }
        );
    }
}
