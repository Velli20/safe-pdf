use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::cff::parser::DictToken;

#[derive(Clone, Copy, PartialEq, Debug, FromPrimitive)]
enum TopDictOperator {
    Version = 0,
    Notice,
    FullName,
    FamilyName,
    Weight,
    FontBBox,
    UniqueID = 13,
    Xuid,
    Charset = 15,
    Encoding,
    CharStrings,
    Private,
    Copyright = (12 << 8),
    IsFixedPitch,
    ItalicAngle,
    UnderlinePosition,
    UnderlineThickness,
    PaintType,
    CharstringType,
    FontMatrix,
    StrokeWidth,
    SyntheticBase = (12 << 8 | 20),
    PostScript,
    BaseFontName,
    BaseFontBlend,

    // CFF spec, "Table 10 CIDFont Operator Extensions"
    RegistryOrderingSupplement = (12 << 8 | 30),
    CIDFontVersion,
    CIDFontRevision,
    CIDFontType,
    CIDCount,
    UIDBase,
    FDArray,
    FDSelect,
    FontName,
}

#[derive(Debug, Default)]
pub(crate) struct TopDictEntry {
    pub char_strings_offset: Option<u16>,
    pub charset_offset: Option<u16>,
    pub encoding: Option<u16>,
}

impl TopDictEntry {
    pub(crate) fn from_dictionary_tokens(operators: &[DictToken]) -> TopDictEntry {
        let mut stack = Vec::new();
        let mut top_dictionary = TopDictEntry::default();

        for op in operators {
            match op {
                DictToken::Operator(b) => {
                    if let Some(op) = TopDictOperator::from_u16(*b) {
                        match op {
                            TopDictOperator::Encoding => {
                                if let Some(DictToken::Number(offset)) = stack.pop() {
                                    // Charset offset is relative to the start of the top dict data
                                    top_dictionary.encoding = Some(offset as u16);
                                } else {
                                    panic!()
                                }
                            }

                            TopDictOperator::Charset => {
                                if let Some(DictToken::Number(offset)) = stack.pop() {
                                    // Charset offset is relative to the start of the top dict data
                                    if offset >= 0 && offset <= u16::MAX as i32 {
                                        top_dictionary.charset_offset = Some(offset as u16);
                                    }
                                } else {
                                    panic!()
                                }
                            }

                            TopDictOperator::CharStrings => {
                                if let Some(DictToken::Number(offset)) = stack.pop() {
                                    // CharStrings offset is relative to the start of the top dict data
                                    if offset >= 0 && offset <= u16::MAX as i32 {
                                        top_dictionary.char_strings_offset = Some(offset as u16);
                                    }
                                } else {
                                    panic!()
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => stack.push(op.clone()),
            }
        }

        top_dictionary
    }
}
