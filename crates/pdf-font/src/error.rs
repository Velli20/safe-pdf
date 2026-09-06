use pdf_cmap::error::CMapError;
use pdf_content_stream_operators::error::PdfOperatorError;
use pdf_object_reader::object_error::ObjectError;
use thiserror::Error;

use crate::base_encoding::BaseEncoding;
use crate::glyph_widths_map::GlyphWidthsMapError;

/// Defines errors that can occur while reading a font object.
#[derive(Debug, Error, PartialEq)]
pub enum FontError {
    #[error("no font driver is registered for {format:?}")]
    DriverUnavailable {
        format: crate::font::FontProgramFormat,
    },
    #[error("invalid {format:?} font program: {message}")]
    InvalidProgram {
        format: crate::font::FontProgramFormat,
        message: String,
    },
    #[error("font collection does not contain face index {face_index}")]
    MissingFace { face_index: u32 },
    #[error("font face {face_id:?} does not contain glyph {glyph_id:?}")]
    MissingGlyph {
        face_id: crate::font::FontFaceId,
        glyph_id: crate::font::GlyphId,
    },
    #[error("font fallback candidates were exhausted")]
    FallbackExhausted,
    #[error("invalid PDF font specification: {message}")]
    InvalidPdfSpecification { message: String },
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
    #[error("unsupported text encoding {0:?}")]
    UnsupportedTextEncoding(BaseEncoding),
    #[error("character {character:?} is not representable in WinAnsi")]
    UnsupportedWinAnsiCharacter { character: char },
    #[error("byte 0x{byte:02X} is undefined in WinAnsi")]
    InvalidWinAnsiByte { byte: u8 },
    #[error("{0}")]
    CMapError(#[from] CMapError),
}

impl From<FontError> for pdf_object_reader::ObjectReadError {
    fn from(source: FontError) -> Self {
        Self::Decode {
            target: "PDF font",
            source: Box::new(source),
        }
    }
}
