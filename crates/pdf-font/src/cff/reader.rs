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

        println!("Len Name INDEX: {}", name_index.len());
        println!("Len Top DICT INDEX: {}", top_dict_index.len());
        println!("Len String INDEX: {}", string_index.len());
        println!("Len Global Subr INDEX: {}", global_subr_index.len());

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
        println!(" CharStrings INDEX: '{}'", char_strings_offset);
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
        })
    }
}

impl CffFontProgram<'_> {
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
