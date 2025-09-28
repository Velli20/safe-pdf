use crate::cff::{
    char_string_operator::char_strings_from, charset::Charset, cursor::Cursor, encoding::Encoding,
    error::CompactFontFormatError, parser::parse_index, program::CffFontProgram,
    top_dictionary_entry::TopDictEntry,
};

pub struct CffFontReader<'a> {
    cursor: Cursor<'a>,
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

        // Read Name index
        let _ = parse_index(&mut self.cursor)?;
        // Read Top DICT index
        let top_dict_index = parse_index(&mut self.cursor)?;

        // Parse operators from the Top DICT table (use the first entry as the main Top DICT)
        let Some(&top_dict_bytes) = top_dict_index.first() else {
            return Err(CompactFontFormatError::InvalidData("missing Top DICT"));
        };

        let dict = TopDictEntry::read(top_dict_bytes)?;

        let Some(char_strings_offset) = dict.char_strings_offset else {
            return Err(CompactFontFormatError::InvalidData(
                "missing CharStrings offset",
            ));
        };

        self.cursor.set_pos(usize::from(char_strings_offset));
        let char_strings_index = parse_index(&mut self.cursor)?;

        let mut char_string_operators = Vec::new();
        for sl in char_strings_index {
            let ops = char_strings_from(sl)?;
            char_string_operators.push(ops);
        }

        let Some(charset_offset) = dict.charset_offset else {
            return Err(CompactFontFormatError::InvalidData(
                "missing charset offset",
            ));
        };

        // If the offset is greater than 2, we have to parse the charset table.
        self.cursor.set_pos(usize::from(charset_offset));
        let charset = Charset::read(&mut self.cursor, char_string_operators.len())?;

        let encoding = Encoding::from_charset(&charset, dict.encoding.unwrap_or(0))?;

        Ok(CffFontProgram {
            char_string_operators,
            encoding,
        })
    }
}
