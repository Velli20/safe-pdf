use core::panic;
use std::collections::HashMap;

use crate::cff::{
    char_string_operator::{CharStringOperatorTrait, char_strings_from},
    cursor::Cursor,
    error::CompactFontFormatError,
    parser::{parse_dict, parse_index},
    top_dictionary_operator::TopDictEntry,
};

/// Represents a String Identifier (SID) in CFF fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SID(pub u16);
#[derive(Debug, Clone, PartialEq)]
pub enum Charset {
    /// Format 0: Sequential list of SIDs
    SID(Vec<SID>),
}

impl Charset {
    /// Build a direct SID -> GID map for faster repeated lookups.
    /// GID is the index into the CharStrings INDEX; SID is the String Identifier.
    pub(crate) fn build_sid_gid_map(&self) -> HashMap<u16, u16> {
        match self {
            Charset::SID(sids) => sids
                .iter()
                .enumerate()
                .filter_map(|(gid, sid)| u16::try_from(gid).ok().map(|g| (sid.0, g)))
                .collect(),
        }
    }
}
pub enum Encoding {
    Standard(HashMap<u16, u16>),
}

impl Encoding {
    /// Construct a Standard encoding map (code point -> GID) given a charset.
    ///
    /// For each 0..=255 code point we look up its Standard Encoding SID and map it
    /// to a GID via the charset. If the SID isn't present (common in subset fonts)
    /// it falls back to GID 0 (.notdef) per the CFF specification.
    pub fn from_charset(charset: &Charset) -> Self {
        let mut mapping = HashMap::new();
        let sid_gid = charset.build_sid_gid_map();
        let notdef_gid = 0u16; // Per spec .notdef is always GID 0
        for code_point in 0u16..=255 {
            let sid = STANDARD_ENCODING[usize::from(code_point)];
            let gid = sid_gid.get(&u16::from(sid)).cloned().unwrap_or(notdef_gid);
            mapping.insert(code_point, gid);
        }
        Encoding::Standard(mapping)
    }
}

pub struct CffFontReader<'a> {
    cursor: Cursor<'a>,
}

pub struct CffFontProgram {
    pub char_string_operators: Vec<Vec<Box<dyn CharStringOperatorTrait>>>,
    pub charset: Charset,
    pub encoding: Encoding,
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
            sids.push(SID(0)); // .notdef
            for _ in 1..number_of_glyphs {
                let sid = cur.read_u16()?;
                sids.push(SID(sid));
            }
            Ok(Charset::SID(sids))
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

    pub fn read_font_program(&mut self) -> Result<CffFontProgram, CompactFontFormatError> {
        // Read header
        let _major = self.cursor.read_u8()?;
        let _minor = self.cursor.read_u8()?;
        let header_size = self.cursor.read_u8()?;

        // Skip to end of header
        self.cursor.set_pos(usize::from(header_size));

        // Read Name INDEX
        let _ = parse_index(&mut self.cursor)?;
        // Read Top DICT INDEX
        let top_dict_index = parse_index(&mut self.cursor)?;

        // Parse operators from the Top DICT table (use the first entry as the main Top DICT)
        let operators = if let Some(&top_dict_bytes) = top_dict_index.first() {
            parse_dict(top_dict_bytes)?
        } else {
            // No Top DICT present; return empty operators for robustness
            Vec::new()
        };

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
            Some(offset) => {
                // If the offset is greater than 2, we have to parse the charset table.
                self.cursor.set_pos(usize::from(offset));
                parse_charset(&mut self.cursor, usize::from(number_of_glyphs))?
            }
            None => panic!(),
        };

        match dict.encoding {
            Some(0) | None => Encoding::Standard,
            d => panic!("Unsupported encoding: {:?}", d),
        };

        let encoding = Encoding::from_charset(&charset);

        Ok(CffFontProgram {
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

impl CffFontProgram {
    pub fn code_to_gid(&self, code_point: u8) -> Option<u16> {
        match &self.encoding {
            Encoding::Standard(mapping) => {
                let code_point = u16::from(code_point);
                mapping.get(&code_point).cloned()
            }
        }
    }
}
