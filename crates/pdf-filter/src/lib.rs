//! PDF stream filter decoding.
//!
//! This crate implements the decompression filters defined in the PDF
//! specification (§7.4):
//!
//! - **FlateDecode** — zlib/deflate (RFC 1950 / RFC 1951)
//! - **DCTDecode** — baseline JPEG
//! - **JPXDecode** — JPEG 2000
//! - **CCITTFaxDecode** — Group 3 / Group 4 fax compression
//! - **ASCII85Decode** — ASCII base-85 encoding
//!
//! The main entry point is [`filter::decode`], which accepts a
//! [`StreamObject`](pdf_object::stream::StreamObject) and applies the full
//! filter chain declared in its `/Filter` dictionary entry.

mod bitreader;
pub mod ccitt;
pub mod ccitt_fax_params;
mod ccitt_tables;
pub mod error;
pub mod filter;
