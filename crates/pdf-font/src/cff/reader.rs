use crate::cff::{
    char_string_operator::{CharStringOperatorTrait, char_strings_from},
    cursor::Cursor,
    error::CompactFontFormatError,
    parser::{parse_dict, parse_index},
    top_dictionary_operator::TopDictEntry,
};

pub struct CffFontReader<'a> {
    cursor: Cursor<'a>,
}

pub struct CffFontProgram<'a> {
    pub header: CffHeader,
    pub name_index: Vec<&'a [u8]>,
    pub top_dict_index: Vec<&'a [u8]>,
    pub string_index: Vec<&'a [u8]>,
    pub global_subr_index: Vec<&'a [u8]>,
    char_string_operators: Vec<Vec<Box<dyn CharStringOperatorTrait>>>,
}

#[derive(Debug)]
pub struct CffHeader {
    pub major: u8,
    pub minor: u8,
    pub header_size: u8,
    pub offset_size: u8,
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
        })
    }
}

impl CffFontProgram<'_> {
    /// Returns the parsed CharString operators for a given glyph ID (GID).
    ///
    /// Notes:
    /// - In CFF, GID 0 is usually .notdef.
    /// - If you have a character code (from text) rather than a GID, you must first map
    ///   code -> SID via the font Encoding, then SID -> GID via the font charset. This
    ///   reader currently exposes the raw CharStrings in order; consumers can add
    ///   Encoding/charset parsing and call this with the resolved GID.
    pub(crate) fn charstring_ops_for_gid(
        &self,
        gid: u8,
    ) -> Option<&[Box<dyn CharStringOperatorTrait>]> {
        self.char_string_operators
            .get(usize::from(gid))
            .map(|ops| ops.as_slice())
    }

    /// Convenient accessor for total number of CharStrings (glyphs) available.
    pub fn glyph_count(&self) -> usize {
        self.char_string_operators.len()
    }
}
