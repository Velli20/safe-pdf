use core::panic;

use crate::cff::{
    char_string_operator::{CharStringOperatorTrait, char_strings_from},
    cursor::Cursor,
    error::CompactFontFormatError,
    parser::{parse_dict, parse_index},
    standard_strings::STANDARD_STRINGS,
    top_dictionary_operator::TopDictEntry,
};

/// Represents a String Identifier (SID) in CFF fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SID(pub u16);
#[derive(Debug, Clone, PartialEq)]
pub enum Charset {
    IsoAdobe,
    Expert,
    ExpertSubset,
    /// Format 0: Sequential list of SIDs
    SID(Vec<SID>),
    /// Format 1/2: Range-based encoding
    SIDRange(Vec<(SID, u16)>),
}

impl Charset {
    pub(crate) fn sid_to_gid(&self, sid: u16) -> Option<u16> {
        if sid == 0 {
            return Some(0);
        }

        match self {
            Charset::IsoAdobe => {
                panic!()
            }
            Charset::Expert => {
                panic!()
            }

            Charset::ExpertSubset => {
                panic!()
            },

            Charset::SID(sids) => sids.iter().position(|n| n.0 == sid).map(|n| (n as u16 + 1)),
            Charset::SIDRange(ranges) => {
                let mut gid = 0u16;
                for (first_sid, n_left) in ranges {
                    let last_sid = first_sid.0 + n_left;
                    if sid >= first_sid.0 && sid <= last_sid {
                        return Some(gid + (sid - first_sid.0));
                    }
                    gid = gid.checked_add(n_left + 1)?;
                }
                panic!();
                None
            }
        }
    }
}
pub enum Encoding {
    Standard,
    Expert,
    Custom(Vec<u8>),
}

pub struct CffFontReader<'a> {
    cursor: Cursor<'a>,
}

pub struct CffFontProgram<'a> {
    pub header: CffHeader,
    pub name_index: Vec<&'a [u8]>,
    pub top_dict_index: Vec<&'a [u8]>,
    pub string_index: Vec<&'a [u8]>,
    pub global_subr_index: Vec<&'a [u8]>,
    pub char_string_operators: Vec<Vec<Box<dyn CharStringOperatorTrait>>>,
    pub charset: Charset,
    pub encoding: Encoding,
}

#[derive(Debug)]
pub struct CffHeader {
    pub major: u8,
    pub minor: u8,
    pub header_size: u8,
    pub offset_size: u8,
}

pub(crate) fn parse_charset<'a>(
    cur: &mut Cursor<'a>,
    number_of_glyphs: usize,
) -> Result<Charset, CompactFontFormatError> {
    let format = cur.read_u8()?;
    // The first high-bit in format indicates that a Supplemental encoding is present.
    // Check it and clear.
    let has_supplemental = format & 0x80 != 0;
    let format = format & 0x7f;
    if has_supplemental {
        panic!("Supplemental encoding not supported");
    }
    match format {
        0 => {
            // Format 0: Sequential list of SIDs
            let mut sids = Vec::new();
            for _ in 0..number_of_glyphs {
                let sid = cur.read_u16()?;
                sids.push(SID(sid));
            }
            Ok(Charset::SID(sids))
        }
        1 | 2 => {
            // Format 1 or 2: Range-based encoding
            let mut ranges = Vec::new();
            for _ in 0..number_of_glyphs {
                let first_sid = cur.read_u16()?;
                let n_left = if format == 1 {
                    u16::from(cur.read_u8()?)
                } else {
                    cur.read_u16()?
                };
                ranges.push((SID(first_sid), n_left));
            }
            Ok(Charset::SIDRange(ranges))
        }
        _ => Err(CompactFontFormatError::InvalidData(
            "invalid charset format",
        )),
    }
}

impl<'a> CffFontReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(data),
        }
    }

    pub fn read_font_program(&mut self) -> Result<CffFontProgram<'a>, CompactFontFormatError> {
        // Read header
        let major = self.cursor.read_u8()?;
        let minor = self.cursor.read_u8()?;
        let header_size = self.cursor.read_u8()?;
        let offset_size = self.cursor.read_u8()?;

        // Skip to end of header
        self.cursor.set_pos(usize::from(header_size));

        // Read Name INDEX
        let name_index = parse_index(&mut self.cursor)?;
        // Read Top DICT INDEX
        let top_dict_index = parse_index(&mut self.cursor)?;
        // Read String INDEX
        let string_index = parse_index(&mut self.cursor)?;
        // Read Global Subr INDEX
        let global_subr_index = parse_index(&mut self.cursor)?;

        // Parse operators from the Top DICT table (use the first entry as the main Top DICT)
        let operators = if let Some(&top_dict_bytes) = top_dict_index.first() {
            parse_dict(top_dict_bytes)?
        } else {
            // No Top DICT present; return empty operators for robustness
            Vec::new()
        };

        if top_dict_index.len() > 1 {
            panic!()
        }
        let dict = TopDictEntry::from_dictionary_tokens(&operators);

        let char_strings_offset =
            dict.char_strings_offset
                .ok_or(CompactFontFormatError::InvalidData(
                    "missing CharStrings offset",
                ))?;
        self.cursor.set_pos(usize::from(char_strings_offset));
        let char_strings_index = parse_index(&mut self.cursor)?;

        let mut char_string_operators = Vec::new();
        for sl in char_strings_index {
            let ops = char_strings_from(sl)?;
            char_string_operators.push(ops);
        }

        // 'The number of glyphs is the value of the count field in the CharStrings INDEX.'
        let number_of_glyphs = u16::try_from(char_string_operators.len())
            .map_err(|_| CompactFontFormatError::InvalidData("todo"))?;

        let charset = match dict.charset_offset {
            Some(0) => Charset::IsoAdobe,
            Some(1) => Charset::Expert,
            Some(2) => Charset::ExpertSubset,
            Some(offset) => {
                self.cursor.set_pos(usize::from(offset));
                parse_charset(&mut self.cursor, usize::from(number_of_glyphs))?
            }
            None => Charset::IsoAdobe, // default
        };

        let encoding = match dict.encoding {
            Some(0) => Encoding::Standard,
            Some(1) => Encoding::Expert,
            Some(_) => todo!("Custom Encoding"),
            None => Encoding::Standard,
        };

        Ok(CffFontProgram {
            header: CffHeader {
                major,
                minor,
                header_size,
                offset_size,
            },
            name_index,
            top_dict_index,
            string_index,
            global_subr_index,
            char_string_operators,
            charset,
            encoding,
        })
    }
}

/// This maps font specific character codes to string ids in a charset.
///
/// See "Glyph Organization" at <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=18>
/// for an explanation of how charsets, encodings and glyphs are related.
///
/// See "Standard" encoding at <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=37>
/// for this particular mapping.
#[rustfmt::skip]
pub const STANDARD_ENCODING: [u8; 256] = [
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      1,   2,   3,   4,   5,   6,   7,   8,   9,  10,  11,  12,  13,  14,  15,  16,
     17,  18,  19,  20,  21,  22,  23,  24,  25,  26,  27,  28,  29,  30,  31,  32,
     33,  34,  35,  36,  37,  38,  39,  40,  41,  42,  43,  44,  45,  46,  47,  48,
     49,  50,  51,  52,  53,  54,  55,  56,  57,  58,  59,  60,  61,  62,  63,  64,
     65,  66,  67,  68,  69,  70,  71,  72,  73,  74,  75,  76,  77,  78,  79,  80,
     81,  82,  83,  84,  85,  86,  87,  88,  89,  90,  91,  92,  93,  94,  95,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,  96,  97,  98,  99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110,
      0, 111, 112, 113, 114,   0, 115, 116, 117, 118, 119, 120, 121, 122,   0, 123,
      0, 124, 125, 126, 127, 128, 129, 130, 131,   0, 132, 133,   0, 134, 135, 136,
    137,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0, 138,   0, 139,   0,   0,   0,   0, 140, 141, 142, 143,   0,   0,   0,   0,
      0, 144,   0,   0,   0, 145,   0,   0, 146, 147, 148, 149,   0,   0,   0,   0,
];

impl CffFontProgram<'_> {
    pub fn code_to_gid(&self, code_point: u8) -> Option<u16> {
        let index = usize::from(code_point);

        match &self.encoding {
            Encoding::Standard => {
                let sid = u16::from(STANDARD_ENCODING[index]);
                self.charset.sid_to_gid(sid)
            }
            Encoding::Expert => {
                panic!()
            }
            Encoding::Custom(_data) => {
                panic!()
            }
        }
    }
    // pub fn glyph_name(&self, glyph_id: u16) -> Option<&'a str> {
    //     // match self.kind {
    //     //     FontKind::SID(_) => {
    //             let sid = self.charset.gid_to_sid(glyph_id)?;
    //             let sid = usize::from(sid.0);
    //             match STANDARD_STRINGS.get(sid) {
    //                 Some(name) => Some(name),
    //                 None => {
    //                     panic!()
    //                     // let idx = u32::try_from(sid - STANDARD_STRINGS.len()).ok()?;
    //                     // let name = self.strings.get(idx)?;
    //                     // core::str::from_utf8(name).ok()
    //                 }
    //             }
    //     //     FontKind::CID(_) => None,
    //     // }
    // }
}
