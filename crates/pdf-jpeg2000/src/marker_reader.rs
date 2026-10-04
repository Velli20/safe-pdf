//! Borrowed JPEG 2000 marker iteration.

use crate::{Jpeg2000Error, marker_site::MarkerSite, offset_site::OffsetSite};
use thiserror::Error;

/// Structural failures while framing codestream markers.
#[derive(Debug, Error)]
pub enum MarkerError {
    /// A marker prefix or code is missing.
    #[error("missing marker at byte {0}")]
    Prefix(usize),
    /// A marker code is reserved for byte stuffing.
    #[error("invalid marker {code:#06x} at byte {offset}")]
    Code { offset: usize, code: u16 },
    /// A length-bearing marker has no complete length field.
    #[error("truncated marker length at byte {0}")]
    Length(usize),
    /// A marker segment length is shorter than its own length field.
    #[error("short marker segment {code:#06x} at byte {offset}")]
    ShortSegment { offset: usize, code: u16 },
    /// A marker segment extends past the input.
    #[error("truncated marker payload at byte {0}")]
    Payload(usize),
    /// A marker position cannot be represented on this target.
    #[error("marker offset overflow")]
    Overflow,
}

/// First byte of every codestream marker, per Annex A.
const MARKER_PREFIX: u8 = 0xff;
/// Byte-stuffed data byte, which is not a marker.
const STUFFED_BYTE: u16 = 0xff00;
/// Fill bytes, which are not a marker segment.
const FILL_BYTES: u16 = 0xffff;
/// Bytes in a marker code.
const MARKER_BYTES: usize = 2;
/// Bytes in the length field of a non-delimiting marker segment.
const SEGMENT_LENGTH_BYTES: usize = 2;
/// Start of codestream marker.
pub(crate) const SOC: u16 = 0xff4f;
/// Image and tile size marker.
pub(crate) const SIZ: u16 = 0xff51;
/// Default coding style marker.
const COD: u16 = 0xff52;
/// Component coding style marker.
const COC: u16 = 0xff53;
/// Tile-part length table marker.
const TLM: u16 = 0xff55;
/// Profile marker.
const PRF: u16 = 0xff56;
/// Main-header packet length table marker.
const PLM: u16 = 0xff57;
/// Tile-part packet length table marker.
const PLT: u16 = 0xff58;
/// Default quantization marker.
const QCD: u16 = 0xff5c;
/// Component quantization marker.
const QCC: u16 = 0xff5d;
/// Region of interest marker.
const RGN: u16 = 0xff5e;
/// Progression order change marker.
const POC: u16 = 0xff5f;
/// Main-header packed packet headers marker.
const PPM: u16 = 0xff60;
/// Tile-part packed packet headers marker.
const PPT: u16 = 0xff61;
/// Component registration marker.
const CRG: u16 = 0xff63;
/// Comment marker.
const COM: u16 = 0xff64;
/// Start of tile-part marker.
const SOT: u16 = 0xff90;
/// Start of packet marker.
const SOP: u16 = 0xff91;
/// End of packet header marker.
const EPH: u16 = 0xff92;
/// Start of tile-part data marker.
const SOD: u16 = 0xff93;
/// End of codestream marker.
pub(crate) const EOC: u16 = 0xffd9;
/// Extended capabilities marker.
const CAP: u16 = 0xff50;

/// A JPEG 2000 Part 1 marker code from the codestream syntax.
///
/// Known variants identify the Annex A marker's role; `Unknown` preserves its
/// numeric code so an unsupported extension can be reported safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Marker {
    /// Start of codestream.
    Soc,
    /// Image and tile size.
    Siz,
    /// Coding style default.
    Cod,
    /// Coding style for one component.
    Coc,
    /// Quantization default.
    Qcd,
    /// Quantization for one component.
    Qcc,
    /// Region of interest information.
    Rgn,
    /// Progression order change.
    Poc,
    /// Start of a tile-part.
    Sot,
    /// Start of tile-part compressed data.
    Sod,
    /// End of codestream.
    Eoc,
    /// Tile-part length table.
    Tlm,
    /// Main-header packet length table.
    Plm,
    /// Tile-part packet length table.
    Plt,
    /// Main-header packed packet headers.
    Ppm,
    /// Tile-part packed packet headers.
    Ppt,
    /// Start of packet marker inside compressed data.
    Sop,
    /// End of packet header marker.
    Eph,
    /// Comment segment.
    Com,
    /// Component registration.
    Crg,
    /// Extended capabilities.
    Cap,
    /// Profile marker.
    Prf,
    /// A marker code outside the named Part 1 set.
    Unknown(u16),
}

impl From<u16> for Marker {
    fn from(code: u16) -> Self {
        match code {
            SOC => Self::Soc,
            SIZ => Self::Siz,
            COD => Self::Cod,
            COC => Self::Coc,
            QCD => Self::Qcd,
            QCC => Self::Qcc,
            RGN => Self::Rgn,
            POC => Self::Poc,
            SOT => Self::Sot,
            SOD => Self::Sod,
            EOC => Self::Eoc,
            TLM => Self::Tlm,
            PLM => Self::Plm,
            PLT => Self::Plt,
            PPM => Self::Ppm,
            PPT => Self::Ppt,
            SOP => Self::Sop,
            EPH => Self::Eph,
            COM => Self::Com,
            CRG => Self::Crg,
            CAP => Self::Cap,
            PRF => Self::Prf,
            _ => Self::Unknown(code),
        }
    }
}

impl Marker {
    /// Identifies delimiter markers, which carry no length field.
    fn standalone(self) -> bool {
        matches!(self, Self::Soc | Self::Sod | Self::Eoc | Self::Eph)
    }
}

/// A marker segment borrowing its payload from the codestream.
///
/// Delimiting markers without a length field have an empty payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkerSegment<'a> {
    marker: Marker,
    site: MarkerSite,
    payload: &'a [u8],
}

impl<'a> MarkerSegment<'a> {
    /// Returns the Part 1 marker associated with this segment.
    pub fn marker(&self) -> Marker {
        self.marker
    }

    /// Returns the raw two-byte marker code.
    pub fn code(&self) -> u16 {
        self.site.code()
    }

    /// Returns the marker's byte offset in the original input slice.
    pub fn offset(&self) -> usize {
        self.site.offset()
    }

    /// Borrows the marker segment body without copying it.
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }

    /// Returns this segment's marker location for error reporting.
    pub(crate) fn site(&self) -> MarkerSite {
        self.site
    }
}

/// Sequential reader for Part 1 markers in a borrowed byte slice.
///
/// Marker lengths are checked before their borrowed payloads are returned. The
/// reader never walks into compressed packet data on its own; a caller skips a
/// tile-part body with [`MarkerReader::take_bytes`].
#[derive(Debug)]
pub struct MarkerReader<'a> {
    input: &'a [u8],
    remaining: &'a [u8],
    site: OffsetSite,
}

impl<'a> MarkerReader<'a> {
    /// Creates a reader at the start of borrowed codestream bytes.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            input: bytes,
            remaining: bytes,
            site: OffsetSite::new(0),
        }
    }

    /// Creates a reader whose reported positions are relative to a container.
    pub(crate) fn new_at(bytes: &'a [u8], base_offset: usize) -> Self {
        Self {
            input: bytes,
            remaining: bytes,
            site: OffsetSite::new(base_offset),
        }
    }

    /// Returns the byte offset at which the next marker would be read.
    pub fn position(&self) -> usize {
        self.site.offset()
    }

    /// Returns the current position for structural diagnostics.
    pub(crate) fn site(&self) -> OffsetSite {
        self.site
    }

    /// Returns bytes already consumed from this reader's borrowed slice.
    pub(crate) fn consumed(&self) -> usize {
        self.input.len().saturating_sub(self.remaining.len())
    }

    /// Borrows the bytes this reader has not consumed yet.
    pub(crate) fn remaining(&self) -> &'a [u8] {
        self.remaining
    }

    /// Consumes compressed bytes that carry no marker syntax.
    ///
    /// Packet bodies are framed by declared lengths, so they are skipped
    /// without inspecting them for marker-looking byte pairs.
    ///
    /// # Errors
    ///
    /// Returns `Truncated` if fewer than `len` bytes remain.
    pub(crate) fn take_bytes(&mut self, len: usize) -> Result<&'a [u8], Jpeg2000Error> {
        let (body, rest) = self
            .remaining
            .split_at_checked(len)
            .ok_or_else(|| self.site.truncated("tile-part body"))?;
        self.site = self
            .site
            .checked_advance(len)
            .ok_or(Jpeg2000Error::Overflow {
                context: "tile-part body length",
            })?;
        self.remaining = rest;
        Ok(body)
    }

    /// Returns the next validated marker segment, or `None` at end of input.
    ///
    /// # Errors
    ///
    /// Returns a structural error if the marker or its declared body is invalid.
    pub fn next_segment(&mut self) -> Result<Option<MarkerSegment<'a>>, Jpeg2000Error> {
        Ok(self.read_next()?)
    }

    /// Reads a marker that must be present at the current position.
    pub(crate) fn next_required(&mut self) -> Result<MarkerSegment<'a>, Jpeg2000Error> {
        self.next_segment()?
            .ok_or_else(|| self.site.truncated("codestream header"))
    }

    /// Reads the required marker at the current position.
    pub(crate) fn require(&mut self, expected: Marker) -> Result<MarkerSegment<'a>, Jpeg2000Error> {
        let segment = self.next_required()?;
        if segment.marker == expected {
            Ok(segment)
        } else {
            segment.site.out_of_order()
        }
    }

    /// Advances one syntax marker, leaving compressed packet data untouched.
    fn read_next(&mut self) -> Result<Option<MarkerSegment<'a>>, MarkerError> {
        if self.remaining.is_empty() {
            return Ok(None);
        }
        let site = self.site;
        let [MARKER_PREFIX, code, rest @ ..] = self.remaining else {
            return site.prefix();
        };
        let code = u16::from_be_bytes([MARKER_PREFIX, *code]);
        let marker_site = site.marker(code);
        if matches!(code, STUFFED_BYTE | FILL_BYTES) {
            return marker_site.invalid_code();
        }
        let marker = Marker::from(code);
        let (payload, consumed): (&[u8], usize) = if marker.standalone() {
            (&[], MARKER_BYTES)
        } else {
            Self::segment_body(rest, marker_site)?
        };
        let rest = self
            .remaining
            .get(consumed..)
            .ok_or_else(|| site.payload())?;
        let next_site = site
            .checked_advance(consumed)
            .ok_or(MarkerError::Overflow)?;
        self.remaining = rest;
        self.site = next_site;
        Ok(Some(MarkerSegment {
            marker,
            site: marker_site,
            payload,
        }))
    }

    /// Reads the length-bearing body after a marker code.
    fn segment_body(bytes: &'a [u8], site: MarkerSite) -> Result<(&'a [u8], usize), MarkerError> {
        let offset_site = site.offset_site();
        let [high, low, ..] = bytes else {
            return offset_site.length();
        };
        let length: usize = u16::from_be_bytes([*high, *low]).into();
        if length < SEGMENT_LENGTH_BYTES {
            return site.short_segment();
        }
        let payload = bytes
            .get(SEGMENT_LENGTH_BYTES..length)
            .ok_or_else(|| offset_site.payload())?;
        let consumed = length
            .checked_add(MARKER_BYTES)
            .ok_or(MarkerError::Overflow)?;
        Ok((payload, consumed))
    }
}
