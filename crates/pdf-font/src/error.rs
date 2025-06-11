use crate::characther_map::CMapError;
use crate::glyph_widths_map::GlyphWidthsMapError;
use pdf_object::error::ObjectError as PdfObjectError;
use thiserror::Error;

/// Defines errors that can occur while reading or processing font-related PDF objects.
#[derive(Debug, Error)]
pub enum FontError {
    /// Error originating from pdf_object crate.
    #[error("PDF object error: {0}")]
    Object(#[from] PdfObjectError),

    /// A required dictionary entry was missing.
    #[error("Missing required entry '{entry_name}' in {dictionary_type} dictionary")]
    MissingEntry {
        entry_name: &'static str,
        dictionary_type: &'static str,
    },

    #[error("Missing /FontDescriptor entry")]
    MissingFontDescriptor,

    /// A dictionary entry had an unexpected type.
    #[error(
        "Entry '{entry_name}' in {dictionary_type} dictionary has invalid type: expected {expected_type}, found {found_type}"
    )]
    InvalidEntryType {
        entry_name: &'static str,
        dictionary_type: &'static str,
        expected_type: &'static str,
        found_type: String,
    },

    // --- FontDescriptor specific ---
    /// The `/FontDescriptor` dictionary contains invalid data.
    #[error("Invalid /FontDescriptor data: {0}")]
    InvalidFontDescriptorData(&'static str), // Used by FontDescriptor parsing

    // --- Type0 Font specific ---
    /// Indicates that the required `/DescendantFonts` entry is missing in a Type0 font dictionary.
    /// This entry is an array typically containing one CIDFont dictionary.
    #[error("Missing /DescendantFonts entry in Type0 font")]
    MissingDescendantFonts,
    /// The `/DescendantFonts` array in a Type0 font is empty or invalid.
    #[error("Invalid /DescendantFonts entry in Type0 font: {0}")]
    InvalidDescendantFonts(String),
    /// Failed to resolve or parse the descendant CIDFont from a Type0 font.
    #[error("Error processing descendant CIDFont for Type0 font")]
    DescendantCIDFontError {
        #[source]
        cause: Box<FontError>,
    },

    // --- CMap / ToUnicode specific ---
    /// Error related to the `/ToUnicode` CMap processing (e.g., reference resolution).
    #[error("Error processing /ToUnicode CMap: {0}")]
    ToUnicodeResolution(String),
    /// Indicates an error occurred while parsing a Character Map (CMap) stream.
    #[error("CMap parsing error: {0}")]
    CMapParse(#[from] CMapError),

    // --- Glyph Widths specific ---
    /// Error parsing the /W array for glyph widths.
    #[error("Glyph widths (/W array) parsing error: {0}")]
    GlyphWidthsParse(#[from] GlyphWidthsMapError),

    /// The font subtype is unsupported or invalid for the current parsing context.
    #[error("Unsupported or invalid font subtype '{subtype}' for {font_type} font")]
    UnsupportedFontSubtype {
        subtype: String,
        font_type: &'static str,
    },

    /// Generic font processing error.
    #[error("Font processing error: {0}")]
    Generic(String),
}
