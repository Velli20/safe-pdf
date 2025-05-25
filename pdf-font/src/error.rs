use pdf_object::error::ObjectError;

/// Defines errors that can occur
#[derive(Debug, Clone, PartialEq)]
pub enum FontError {
    /// Wraps an error message from an `ObjectError`
    /// encountered while processing a PDF object related to the page.
    ObjectError(String),
    /// Indicates that the required `/FontDescriptor` dictionary
    /// is missing for a font.
    MissingFontDescriptor,
    /// Indicates that the `/FontDescriptor` dictionary for a font
    /// is present but contains invalid or malformed data.
    InvalidFontDescriptor(&'static str),
    /// Indicates that the required `Subtype` entry is missing for a CID font.
    MissingSubtype,
    /// Indicates that the required `/DescendantFonts` entry is missing in a Type0 font dictionary.
    /// This entry is an array typically containing one CIDFont dictionary.
    MissingDescendantFonts,
    /// Indicates that a Character Identifier (CID) font dictionary, expected as a descendant
    /// of a Type0 font, is missing or could not be processed.
    MissingCharacterIdentifierFont,
    /// Indicates an error occurred while parsing a Character Map stream.
    CMapParseError(String),
}

impl From<ObjectError> for FontError {
    fn from(err: ObjectError) -> Self {
        Self::ObjectError(err.to_string())
    }
}

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontError::ObjectError(err) => {
                write!(
                    f,
                    "Failed to process a PDF object related to the page: {}",
                    err
                )
            }
            FontError::InvalidFontDescriptor(err) => {
                write!(f, "The `/FontDescriptor` entry is invalid: {}", err)
            }
            FontError::MissingSubtype => {
                write!(
                    f,
                    "The required `/Subtype` entry is missing for a CID font."
                )
            }
            FontError::MissingFontDescriptor => write!(f, "Missing `/FontDescriptor` entry"),
            FontError::MissingDescendantFonts => {
                write!(f, "Missing `/DescendantFonts` entry in Type0 font")
            }
            FontError::MissingCharacterIdentifierFont => {
                write!(f, "Missing Character Identifier (CID) font dictionary")
            }
            FontError::CMapParseError(err) => {
                write!(f, "Failed to parse CMap: {}", err)
            }
        }
    }
}
