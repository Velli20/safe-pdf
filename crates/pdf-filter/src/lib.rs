//! PDF stream filter decoding.
//!
//! This crate implements the decompression filters defined in the PDF
//! specification (§7.4):
//!
//! - **FlateDecode** — zlib/deflate (RFC 1950 / RFC 1951)
//! - **LZWDecode** — LZW compression (§7.4.4)
//! - **DCTDecode** — baseline JPEG
//! - **JPXDecode** — JPEG 2000
//! - **CCITTFaxDecode** — Group 3 / Group 4 fax compression
//! - **ASCII85Decode** — ASCII base-85 encoding
//! - **ASCIIHexDecode** — ASCII hexadecimal encoding
//!
//! The main entry point is [`filter::decode`], which accepts a
//! [`StreamObject`](pdf_object::stream::StreamObject) and applies the full
//! filter chain declared in its `/Filter` dictionary entry.

pub(crate) mod ascii85;
pub(crate) mod asciihex;
mod bitreader;
pub(crate) mod ccitt;
pub(crate) mod ccitt_fax_params;
mod ccitt_tables;
pub mod error;
pub mod filter;
pub(crate) mod lzw;
pub(crate) mod predictor;
