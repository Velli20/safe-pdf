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
//! The main entry point is [`filter::decode`], which accepts a
//! [`StreamObject`](pdf_object_reader::stream::StreamObject) and applies the full
//! filter chain declared in its `/Filter` dictionary entry. Callers that hold
//! a dictionary and shared data separately can use
//! [`filter::decode_data_with_resolver`] without constructing a stream object.
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
