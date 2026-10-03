//! Borrowing, resource-bounded API for JPEG 2000 Part 1 images in PDF streams.
//!
//! The public types describe raw JPEG 2000 codestreams and their optional JP2
//! container as specified by ISO/IEC 15444-1. Decoding is complete from the
//! container to reconstructed component samples: box and marker parsing,
//! tile-part framing, Tier-2 packet headers, Tier-1 code-block coefficients,
//! inverse quantization, the inverse wavelet transform, and the inverse
//! multiple-component transform. A JPX container is resolved to the codestream
//! its first compositing layer names, with that layer's and that codestream's
//! own header boxes laid over the file's defaults. [`ImageChannels`] then
//! resolves the container boxes into the output channels a renderer consumes,
//! expanding a palette and naming the opacity channel a PDF `SMaskInData` entry
//! selects. Applying that entry, and converting the named colour space, belong
//! to the PDF image path rather than to this crate.
//!
//! # Borrowing contract
//!
//! Compressed input, marker payloads, and box payloads are borrowed from the
//! caller's slice for the decoder's lifetime, and reconstructed tile samples
//! live only for the duration of a [`TileSink::write_tile`] call. No type in
//! this crate is an `extern` ABI type, so a future FFI or WASM wrapper must
//! retain the input allocation for as long as it holds any view from this
//! crate, and must copy the samples it needs before returning from the sink.
//!
//! A valid state sequence has this shape:
//!
//! ```no_run
//! use pdf_jpeg2000::{Decoder, DecoderLimits, DecoderOptions, InputFormat, Jpeg2000Error, TileSink, TileView};
//!
//! struct Sink;
//! impl TileSink for Sink {
//!     fn write_tile(&mut self, _tile: TileView<'_>) -> Result<(), Jpeg2000Error> {
//!         Ok(())
//!     }
//! }
//!
//! fn decode(input: &[u8], limits: DecoderLimits) -> Result<(), Jpeg2000Error> {
//!     let options = DecoderOptions { format: InputFormat::Auto, limits };
//!     let decoder = Decoder::new(input, options).parse_header()?;
//!     let mut sink = Sink;
//!     let mut decoded = decoder.decode_next_tile(&mut sink)?;
//!     while decoded.remaining_tiles() > 0 {
//!         decoded = decoded.decode_next_tile(&mut sink)?;
//!     }
//!     let _complete = decoded.finish()?;
//!     Ok(())
//! }
//! ```
//!
//! Calling a later transition on the initial state fails to compile:
//!
//! ```compile_fail
//! use pdf_jpeg2000::{Decoder, DecoderLimits, DecoderOptions, InputFormat};
//! fn invalid(input: &[u8], limits: DecoderLimits) {
//!     let options = DecoderOptions { format: InputFormat::Auto, limits };
//!     let _ = Decoder::new(input, options).finish();
//! }
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod codestream;
pub mod container;
pub mod decoder;
pub mod error;
pub mod limits;
pub mod tile;

mod box_reader;
mod byte_image;
mod channel_definition;
mod chunk_source;
mod chunked_bytes;
mod code_block_decoder;
mod code_block_grid;
mod code_block_state;
mod codestream_registration;
mod coding;
mod coefficient;
mod color_space;
mod color_specifications;
mod component_map;
mod component_transform;
mod compositing_layer;
mod composition;
mod context_tables;
mod dequantize;
mod fragment_list;
mod image_channels;
mod jp2;
mod jp2_header;
mod lifting;
mod lifting_53;
mod lifting_97;
mod main_header;
mod marker_reader;
mod marker_site;
mod mq_contexts;
mod offset_site;
mod opacity;
mod packed_headers;
mod packet_header;
mod packet_lengths;
mod palette;
mod pass_cleanup;
mod pass_coder;
mod pass_lengths;
mod pass_refinement;
mod pass_significance;
mod position_scan;
mod precinct;
mod progression;
mod quantization;
mod reader_requirements;
mod region;
mod resolution;
mod size;
mod stuffed_bits;
mod tag_tree;
mod tile_coding;
mod tile_layout;
mod tile_packets;
mod tile_part;
mod tile_reconstruct;
mod tile_structure;
mod wavelet;
mod workspace;

pub use byte_image::ByteImage;
pub use decoder::{
    Complete, DecodeState, Decoder, HeaderParsed, Progress, TileDecoded, Uninitialized,
};
pub use error::{Jpeg2000Error, Resource};
pub use limits::{DecoderLimits, DecoderOptions};
pub use tile::{ComponentPlane, SampleData, TileBounds, TileSink, TileView};

pub use container::{
    ChannelAssociation, ChannelDefinition, ChannelDefinitions, ChannelKind, ChannelSource,
    CodestreamAssociation, CodestreamRegistration, CodestreamRegistrationRecord, ColorDescription,
    ColorSpecification, ColorSpecifications, ComponentMapping, CompositingLayer, Composition,
    EnumeratedColorSpace, Fragment, FragmentList, ImageChannels, InputFormat, Opacity, OpacityKind,
    OutputChannel, Palette, PaletteColumn, ReaderRequirements,
};
