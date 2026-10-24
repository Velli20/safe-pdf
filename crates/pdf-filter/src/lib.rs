//! PDF stream filter decoding.
//!
//! This crate implements the decompression filters defined in the PDF
//! specification (§7.4):
//!
//! - **FlateDecode** — zlib/deflate (RFC 1950 / RFC 1951)
//! - **LZWDecode** — LZW compression (§7.4.4)
//! - **DCTDecode** — baseline JPEG
//! - **JPXDecode** — JPEG 2000 (framed only; see [`image_payload::ImagePayload`])
//! - **CCITTFaxDecode** — Group 3 / Group 4 fax compression
//! - **ASCII85Decode** — ASCII base-85 encoding
//! - **ASCIIHexDecode** — ASCII hexadecimal encoding
//! - **RunLengthDecode** — PDF run-length encoding
//!
//! Streams are decoded by reading [`filter::DecodedStream`] or
//! [`image_payload::ImagePayload`] through an object reader, which applies the full
//! filter chain declared in the stream's `/Filter` entry and resolves indirect filter
//! entries. [`filter::decode`] does the same for a stream with only direct entries.
//!
//! `JPXDecode` is the one filter this crate does not turn into samples. A JPEG
//! 2000 codestream describes its own components, precision, colour space, and
//! opacity, so [`image_payload::ImagePayload`] applies the filters preceding it
//! and hands the codestream to the image path with that metadata still
//! available.

pub(crate) mod ascii85;
pub(crate) mod asciihex;
pub mod error;
pub mod filter;
pub mod image_payload;
pub(crate) mod jpeg_frame;
pub(crate) mod lzw;
pub(crate) mod predictor;
pub(crate) mod runlength;
