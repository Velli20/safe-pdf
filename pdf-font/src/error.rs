use crate::characther_map::CMapError;
use thiserror::Error;

/// Defines errors that can occur while reading a Font.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum FontError {
    /// Wraps an error message from an `ObjectError`
    /// encountered while processing a PDF object related to the page.
    #[error("Failed to process a PDF object related to the font: {0}")]
    ObjectError(#[from] pdf_object::error::ObjectError),
    /// Indicates that the required `/FontDescriptor` dictionary
    /// is missing for a font.
    #[error("Missing /FontDescriptor entry")]
    MissingFontDescriptor,
    /// Indicates that the `/FontDescriptor` dictionary for a font
    /// is present but contains invalid or malformed data.
    #[error("The /FontDescriptor entry is invalid: {0}")]
    InvalidFontDescriptor(&'static str),
    /// Indicates that the required `Subtype` entry is missing for a CID font.
    #[error("The required /Subtype entry is missing for a CID font")]
    MissingSubtype,
    /// Indicates that the required `/DescendantFonts` entry is missing in a Type0 font dictionary.
    /// This entry is an array typically containing one CIDFont dictionary.
    #[error("Missing /DescendantFonts entry in Type0 font")]
    MissingDescendantFonts,
    /// Indicates that a Character Identifier (CID) font dictionary, expected as a descendant
    /// of a Type0 font, is missing or could not be processed.
    #[error("Missing Character Identifier (CID) font dictionary")]
    MissingCharacterIdentifierFont,
    /// Indicates an error occurred while parsing a Character Map (CMap) stream.
    #[error("Failed to parse CMap")]
    CMapParseError(#[from] CMapError),
}
