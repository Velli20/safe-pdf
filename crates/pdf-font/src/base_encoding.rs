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
