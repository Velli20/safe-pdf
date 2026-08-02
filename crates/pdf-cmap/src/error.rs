use thiserror::Error;

/// Errors that can occur while parsing or resolving PDF CMaps.
#[derive(Debug, Error, PartialEq)]
pub enum CMapError {
    #[error("Object error while reading a CMap: {0}")]
    ObjectError(#[from] pdf_object::error::ObjectError),
    #[error("Unsupported Type0 /Encoding CMap '{0}'")]
    UnsupportedType0EncodingCMap(String),
    #[error("Invalid Type0 /Encoding CMap: {0}")]
    InvalidType0EncodingCMap(String),
    #[error("Invalid CMap u16 bytes")]
    InvalidCMapU16Bytes,
    #[error("Invalid CMap u32 bytes")]
    InvalidCMapU32Bytes,
    #[error("Invalid CMap u16 integer")]
    InvalidCMapU16Integer,
    #[error("Unknown CMap keyword '{0}'")]
    UnknownCMapKeyword(String),
    #[error("{0}")]
    ParserError(#[from] pdf_parser::error::ParserError),
}
