use pdf_content_stream::error::PdfOperatorError;
use pdf_object::error::ObjectError;
use thiserror::Error;

use crate::glyph_widths_map::GlyphWidthsMapError;

/// Defines errors that can occur while reading a font object.
#[derive(Debug, Error, PartialEq)]
pub enum FontError {
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("Unsupported or invalid font subtype '{subtype}'")]
    UnsupportedFontSubtype { subtype: String },
    #[error("Failed to build CFF font: {0}")]
    FontBuildError(String),
    #[error("Error parsing content stream operators: {0}")]
    ContentStreamError(#[from] PdfOperatorError),
    #[error("Missing embedded font file stream")]
    MissingFontFile,
    #[error("{0}")]
    GlyphWidthsMapError(#[from] GlyphWidthsMapError),
    #[error("Unsupported CIDFont subtype '{subtype}'")]
    UnsupportedCidFontSubtype { subtype: String },
    #[error("Invalid /DescendantFonts entry in Type0 font: {0}")]
    InvalidDescendantFonts(&'static str),
    #[error("Unsupported /BaseEncoding value '{0}'")]
    UnsupportedBaseEncoding(String),
}
