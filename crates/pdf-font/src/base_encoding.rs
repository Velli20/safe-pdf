//! Base encodings for simple PDF fonts.

/// Standard simple-font encoding selected by a PDF name.
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum BaseEncoding {
    /// The font program's built-in encoding.
    #[default]
    BuiltIn,
    /// StandardEncoding from the PDF specification.
    Standard,
    /// MacRomanEncoding from the PDF specification.
    MacRoman,
    /// WinAnsiEncoding from the PDF specification.
    WinAnsi,
    /// MacExpertEncoding from the PDF specification.
    MacExpert,
    /// An unrecognized PDF encoding name.
    Unknown(Vec<u8>),
}

impl From<&[u8]> for BaseEncoding {
    fn from(name: &[u8]) -> Self {
        match name {
            b"MacRomanEncoding" => Self::MacRoman,
            b"MacExpertEncoding" => Self::MacExpert,
            b"StandardEncoding" => Self::Standard,
            b"WinAnsiEncoding" => Self::WinAnsi,
            _ => Self::Unknown(Vec::from(name)),
        }
    }
}

impl BaseEncoding {
    /// Encodes UTF-8 text using the font encoding.
    pub fn encode(&self, text: &str) -> Result<Vec<u8>, crate::error::FontError> {
        text.chars()
            .map(|character| self.encode_char(character))
            .collect()
    }

    /// Encodes a single Unicode scalar value using the font encoding.
    pub fn encode_char(&self, character: char) -> Result<u8, crate::error::FontError> {
        match self {
            Self::WinAnsi => crate::encoding::encode_win_ansi_char(character),
            _ => Err(crate::error::FontError::UnsupportedTextEncoding(
                self.clone(),
            )),
        }
    }

    /// Decodes bytes using the font encoding.
    pub fn decode(&self, bytes: &[u8]) -> Result<String, crate::error::FontError> {
        bytes
            .iter()
            .copied()
            .map(|byte| self.decode_byte(byte))
            .collect()
    }

    /// Decodes a single encoded byte using the font encoding.
    pub fn decode_byte(&self, byte: u8) -> Result<char, crate::error::FontError> {
        match self {
            Self::WinAnsi => crate::encoding::decode_win_ansi_byte(byte),
            _ => Err(crate::error::FontError::UnsupportedTextEncoding(
                self.clone(),
            )),
        }
    }
}
