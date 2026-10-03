//! Checked, borrowed iteration over JP2-family boxes.

use std::fmt;

use crate::{Jpeg2000Error, jp2::ContainerError};
use thiserror::Error;

/// Structural failures while framing JP2-family boxes.
#[derive(Debug, Error)]
pub enum BoxError {
    /// Fewer than eight bytes remain for a box header.
    #[error("truncated box header at byte {0}")]
    Header(usize),
    /// An XLBox field is incomplete.
    #[error("truncated extended box header at byte {0}")]
    ExtendedHeader(usize),
    /// A box extends beyond its enclosing scope.
    #[error("truncated box payload at byte {0}")]
    Payload(usize),
    /// LBox or XLBox is shorter than its header.
    #[error("invalid length for box {kind} at byte {offset}")]
    InvalidLength {
        /// Byte offset of the box header.
        offset: usize,
        /// Box type whose length field is invalid.
        kind: BoxKind,
    },
    /// A declared length or offset is not addressable on this target.
    #[error("box offset or length overflow")]
    Overflow,
}

/// Bytes in a standard JP2 box header.
const BOX_HEADER_LEN: usize = 8;
/// Bytes in a JP2 box header with XLBox.
const EXTENDED_BOX_HEADER_LEN: usize = 16;
/// LBox value indicating that the box fills its enclosing scope.
const BOX_TO_SCOPE_END: u32 = 0;
/// LBox value indicating an XLBox field follows.
const EXTENDED_BOX_LENGTH: u32 = 1;

/// A JP2-family box type this decoder recognises.
///
/// `Other` preserves the four-byte type of a box the decoder does not
/// interpret, so an unknown but permitted box can be reported or skipped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BoxKind {
    /// JPEG 2000 Signature box.
    Signature,
    /// File Type box.
    FileType,
    /// JP2 Header superbox.
    Jp2Header,
    /// Contiguous Codestream box.
    Codestream,
    /// Fragment Table box, whose codestream is assembled from fragments.
    FragmentTable,
    /// Image Header box inside JP2 Header.
    ImageHeader,
    /// Bits Per Component box inside JP2 Header.
    BitsPerComponent,
    /// Colour Specification box inside JP2 Header.
    ColorSpecification,
    /// Palette box inside JP2 Header.
    Palette,
    /// Component Mapping box inside JP2 Header.
    ComponentMapping,
    /// Channel Definition box inside JP2 Header.
    ChannelDefinition,
    /// Reader Requirements box of a JPX file.
    ReaderRequirements,
    /// Codestream Header superbox describing one JPX codestream.
    CodestreamHeader,
    /// Compositing Layer Header superbox describing one JPX layer.
    CompositingLayerHeader,
    /// Colour Group superbox holding a compositing layer's colour boxes.
    ColorGroup,
    /// Opacity box of a JPX compositing layer.
    Opacity,
    /// Codestream Registration box naming a layer's codestreams.
    CodestreamRegistration,
    /// Composition superbox placing JPX compositing layers on a canvas.
    Composition,
    /// Composition Options box giving the composition canvas.
    CompositionOptions,
    /// Composition Instruction Set box.
    CompositionInstruction,
    /// Fragment List box inside a Fragment Table.
    FragmentList,
    /// Data Reference box naming files outside this one.
    DataReference,
    /// A box type outside the recognised set.
    Other([u8; 4]),
}

impl fmt::Display for BoxKind {
    /// Names a recognised box, or quotes an unrecognised four-byte type.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self::Other(tag) = self else {
            return write!(formatter, "{self:?}");
        };
        formatter.write_str("'")?;
        for byte in tag {
            match char::from(*byte) {
                character if character.is_ascii_graphic() || character == ' ' => {
                    write!(formatter, "{character}")?;
                }
                _ => write!(formatter, "\\x{byte:02x}")?,
            }
        }
        formatter.write_str("'")
    }
}

impl From<[u8; 4]> for BoxKind {
    fn from(tag: [u8; 4]) -> Self {
        match &tag {
            b"jP  " => Self::Signature,
            b"ftyp" => Self::FileType,
            b"jp2h" => Self::Jp2Header,
            b"jp2c" => Self::Codestream,
            b"ftbl" => Self::FragmentTable,
            b"ihdr" => Self::ImageHeader,
            b"bpcc" => Self::BitsPerComponent,
            b"colr" => Self::ColorSpecification,
            b"pclr" => Self::Palette,
            b"cmap" => Self::ComponentMapping,
            b"cdef" => Self::ChannelDefinition,
            b"rreq" => Self::ReaderRequirements,
            b"jpch" => Self::CodestreamHeader,
            b"jplh" => Self::CompositingLayerHeader,
            b"cgrp" => Self::ColorGroup,
            b"opct" => Self::Opacity,
            b"creg" => Self::CodestreamRegistration,
            b"comp" => Self::Composition,
            b"copt" => Self::CompositionOptions,
            b"inst" => Self::CompositionInstruction,
            b"flst" => Self::FragmentList,
            b"dtbl" => Self::DataReference,
            _ => Self::Other(tag),
        }
    }
}

/// One borrowed JP2 box payload and its position in the source slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Jp2Box<'a> {
    kind: BoxKind,
    offset: usize,
    payload_offset: usize,
    payload: &'a [u8],
}

impl<'a> Jp2Box<'a> {
    /// Returns the recognised box type from the JP2 box header.
    pub fn kind(&self) -> BoxKind {
        self.kind
    }

    /// Returns the box's starting byte offset in the original input.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Returns the payload's absolute byte offset in the original input.
    pub(crate) fn payload_offset(&self) -> usize {
        self.payload_offset
    }

    /// Borrows the box body without copying it.
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }

    /// Reads child boxes within this box's payload.
    pub fn children(&self) -> Jp2BoxReader<'a> {
        Jp2BoxReader {
            remaining: self.payload,
            position: self.payload_offset,
        }
    }
}

/// Sequential reader for JP2 boxes in the caller's byte slice.
///
/// Short, extended, and scope-ending lengths are checked before returning
/// borrowed [`Jp2Box`] views. The reader never owns the source bytes.
#[derive(Debug)]
pub struct Jp2BoxReader<'a> {
    remaining: &'a [u8],
    position: usize,
}

impl<'a> Jp2BoxReader<'a> {
    /// Creates a reader at the start of a JP2 byte slice.
    ///
    /// Signature and file-type validation belong to the container parser.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            remaining: bytes,
            position: 0,
        }
    }

    /// Returns the byte offset at which the next JP2 box would be read.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Returns the next JP2 box, or `None` at the end of the input.
    ///
    /// # Errors
    ///
    /// Returns a typed box error for invalid framing or an out-of-scope length.
    pub fn next_box(&mut self) -> Result<Option<Jp2Box<'a>>, Jpeg2000Error> {
        Ok(self.read_next()?)
    }

    /// Reads the next box, requiring a specific type at this position.
    ///
    /// # Errors
    ///
    /// Returns `Container` if the box is absent or has another type.
    pub(crate) fn require(&mut self, kind: BoxKind) -> Result<Jp2Box<'a>, Jpeg2000Error> {
        let entry = self.next_box()?.ok_or(ContainerError::Required(kind))?;
        if entry.kind() != kind {
            return Err(ContainerError::Required(kind).into());
        }
        Ok(entry)
    }

    /// Advances one box without changing state when framing fails.
    fn read_next(&mut self) -> Result<Option<Jp2Box<'a>>, BoxError> {
        if self.remaining.is_empty() {
            return Ok(None);
        }
        let offset = self.position;
        let header = BoxHeader::parse(self.remaining, offset)?;
        let box_len = header.length.unwrap_or(self.remaining.len());
        let (box_bytes, rest) = self
            .remaining
            .split_at_checked(box_len)
            .ok_or(BoxError::Payload(offset))?;
        let payload = box_bytes
            .get(header.header_len..)
            .ok_or(BoxError::InvalidLength {
                offset,
                kind: header.kind,
            })?;
        let payload_offset = offset
            .checked_add(header.header_len)
            .ok_or(BoxError::Overflow)?;
        let next_position = offset.checked_add(box_len).ok_or(BoxError::Overflow)?;
        self.remaining = rest;
        self.position = next_position;
        Ok(Some(Jp2Box {
            kind: header.kind,
            offset,
            payload_offset,
            payload,
        }))
    }
}

/// LBox, TBox, and optional XLBox from one box header.
struct BoxHeader {
    kind: BoxKind,
    length: Option<usize>,
    header_len: usize,
}

impl BoxHeader {
    /// Reads the fixed box header and its optional extended length.
    fn parse(bytes: &[u8], offset: usize) -> Result<Self, BoxError> {
        let [a, b, c, d, e, f, g, h, rest @ ..] = bytes else {
            return Err(BoxError::Header(offset));
        };
        let kind = BoxKind::from([*e, *f, *g, *h]);
        let (length, header_len) = match u32::from_be_bytes([*a, *b, *c, *d]) {
            BOX_TO_SCOPE_END => (None, BOX_HEADER_LEN),
            EXTENDED_BOX_LENGTH => (
                Some(Self::extended_length(rest, offset)?),
                EXTENDED_BOX_HEADER_LEN,
            ),
            value => {
                let length = usize::try_from(value).map_err(|_| BoxError::Overflow)?;
                (Some(length), BOX_HEADER_LEN)
            }
        };
        if length.is_some_and(|value| value < header_len) {
            return Err(BoxError::InvalidLength { offset, kind });
        }
        Ok(Self {
            kind,
            length,
            header_len,
        })
    }

    /// Converts an XLBox field to an addressable length for this target.
    fn extended_length(bytes: &[u8], offset: usize) -> Result<usize, BoxError> {
        let [a, b, c, d, e, f, g, h, ..] = bytes else {
            return Err(BoxError::ExtendedHeader(offset));
        };
        let length = u64::from_be_bytes([*a, *b, *c, *d, *e, *f, *g, *h]);
        usize::try_from(length).map_err(|_| BoxError::Overflow)
    }
}
