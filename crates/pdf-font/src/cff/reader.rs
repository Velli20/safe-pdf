use crate::cff::{
    char_string_interpreter::CharStringOperatorTrait, char_string_operator::char_strings_from,
    charset::Charset, cursor::Cursor, encoding::Encoding, error::CompactFontFormatError,
    parser::parse_index, top_dictionary_operator::TopDictEntry,
};

pub struct CffFontReader<'a> {
    cursor: Cursor<'a>,
}

pub struct CffFontProgram {
    pub char_string_operators: Vec<Vec<Box<dyn CharStringOperatorTrait>>>,
    pub charset: Charset,
    pub encoding: Encoding,
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
        let Some(&top_dict_bytes) = top_dict_index.first() else {
            return Err(CompactFontFormatError::InvalidData("missing Top DICT"));
        };

        let dict = TopDictEntry::read(top_dict_bytes)?;

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

        let charset = if let Some(offset) = dict.charset_offset {
            // If the offset is greater than 2, we have to parse the charset table.
            self.cursor.set_pos(usize::from(offset));
            Charset::read(&mut self.cursor, usize::from(number_of_glyphs))?
        } else {
            return Err(CompactFontFormatError::InvalidData(
                "missing charset offset",
            ));
        };

        let encoding = Encoding::from_charset(&charset, dict.encoding.unwrap_or(0))?;

        Ok(CffFontProgram {
            char_string_operators,
            charset,
            encoding,
        })
    }
}

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
