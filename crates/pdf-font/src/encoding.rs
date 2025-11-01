use std::borrow::Cow;

#[derive(PartialEq, Clone)]
pub enum FontEncoding {
    /// Standard Mac Roman encoding.
    MacRoman,
    /// Mac Expert encoding.
    MacExpert,
    /// Standard encoding.
    Standard,
    /// Windows ANSI encoding.
    WinAnsi,
    /// PDF Document encoding.
    PDFDocEncoding,
    /// Unknown encoding.
    Unknown(String),
}

impl From<Cow<'_, str>> for FontEncoding {
    fn from(name: Cow<'_, str>) -> Self {
        match name.as_ref() {
            "MacRomanEncoding" => Self::MacRoman,
            "MacExpertEncoding" => Self::MacExpert,
            "StandardEncoding" => Self::Standard,
            "WinAnsiEncoding" => Self::WinAnsi,
            "PDFDocEncoding" => Self::PDFDocEncoding,
            _ => Self::Unknown(name.to_string()),
        }
    }
}
