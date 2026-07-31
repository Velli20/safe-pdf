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
//! - **RunLengthDecode** — PDF run-length encoding
//!
//! The main entry point is [`filter::decode`], which accepts a
//! [`StreamObject`](pdf_object::stream::StreamObject) and applies the full
//! filter chain declared in its `/Filter` dictionary entry. Callers that hold
//! a dictionary and shared data separately can use
//! [`filter::decode_data_with_resolver`] without constructing a stream object.

pub(crate) mod ascii85;
pub(crate) mod asciihex;
pub mod error;
pub mod filter;
pub(crate) mod lzw;
pub(crate) mod predictor;
pub(crate) mod runlength;
