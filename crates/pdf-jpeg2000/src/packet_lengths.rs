//! Length tables that describe the compressed data without decoding it.
//!
//! Annex A.7 defines three optional tables: TLM lists tile-part lengths in the
//! main header, PLT lists packet lengths in a tile-part header, and PLM lists
//! them in the main header. None of them is needed to decode, so they are used
//! here only as cross-checks: a table that contradicts the codestream it
//! describes marks the file as damaged.
//!
//! PLM associates a variable number of bytes with each tile-part through a
//! one-byte `Nplm` field, and Annex A.7.2 lets one tile-part's list continue
//! into the next segment without saying how a reader re-synchronises. Its
//! framing is therefore checked, but its lengths are not matched against
//! packets.

use crate::{
    Jpeg2000Error,
    codestream::{MainHeader, Marker, MarkerReader, MarkerSegment},
    marker_site::MarkerSite,
    tile_part::TilePartIndex,
    workspace::Workspace,
};

/// Shift selecting the tile-index width from `Stlm`.
const TILE_INDEX_SHIFT: u32 = 4;
/// Mask selecting the tile-index width once shifted.
const TILE_INDEX_MASK: u8 = 0x03;
/// Shift selecting the tile-part length width from `Stlm`.
const LENGTH_WIDTH_SHIFT: u32 = 6;
/// Mask selecting the tile-part length width once shifted.
const LENGTH_WIDTH_MASK: u8 = 0x01;
/// Bytes of a short `Ptlm` field.
const SHORT_LENGTH_BYTES: usize = 2;
/// Bytes of a long `Ptlm` field.
const LONG_LENGTH_BYTES: usize = 4;
/// Continuation bit of a packet length in a PLT or PLM list.
const CONTINUATION: u8 = 0x80;
/// Value bits of one packet-length byte.
const VALUE_MASK: u8 = 0x7f;
/// Value bits contributed by one packet-length byte.
const VALUE_BITS: u32 = 7;

/// Tile-part lengths declared by the main header's TLM segments.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TilePartLengths;

impl TilePartLengths {
    /// Checks every TLM entry against the framed tile-parts.
    ///
    /// # Errors
    ///
    /// Returns `InvalidMarker` if a segment is malformed, out of order, or
    /// disagrees with the tile-part it describes.
    pub(crate) fn verify(
        main: &MainHeader<'_>,
        index: &TilePartIndex<'_>,
    ) -> Result<(), Jpeg2000Error> {
        let mut markers = main.markers();
        let mut sequence = 0u8;
        let mut position = 0usize;
        while let Some(segment) = markers.next_segment()? {
            if segment.marker() != Marker::Tlm {
                continue;
            }
            let entries = TileLengthEntries::parse(segment, &mut sequence)?;
            position = entries.verify(index, position)?;
        }
        Ok(())
    }
}

/// One TLM segment's entry table.
#[derive(Clone, Copy, Debug)]
struct TileLengthEntries<'a> {
    entries: &'a [u8],
    tile_bytes: usize,
    length_bytes: usize,
    site: MarkerSite,
}

impl<'a> TileLengthEntries<'a> {
    /// Reads `Ztlm` and `Stlm` and checks the entry table's shape.
    fn parse(segment: MarkerSegment<'a>, sequence: &mut u8) -> Result<Self, Jpeg2000Error> {
        let site = segment.site();
        let [order, style, entries @ ..] = segment.payload() else {
            return site.reject_invalid("TLM fields are incomplete");
        };
        let entries: &[u8] = entries;
        if *order != *sequence {
            return site.reject_invalid("TLM segments are out of order");
        }
        *sequence = sequence.saturating_add(1);
        let tile_bytes = usize::from((style >> TILE_INDEX_SHIFT) & TILE_INDEX_MASK);
        let length_bytes = if (style >> LENGTH_WIDTH_SHIFT) & LENGTH_WIDTH_MASK == 1 {
            LONG_LENGTH_BYTES
        } else {
            SHORT_LENGTH_BYTES
        };
        let stride = tile_bytes
            .checked_add(length_bytes)
            .ok_or_else(|| site.invalid("TLM entry width overflows"))?;
        if tile_bytes > 2 || stride == 0 || !entries.len().is_multiple_of(stride) {
            return site.reject_invalid("TLM entry table does not match Stlm");
        }
        Ok(Self {
            entries,
            tile_bytes,
            length_bytes,
            site,
        })
    }

    /// Compares this segment's entries with the tile-parts they describe.
    fn verify(self, index: &TilePartIndex<'_>, start: usize) -> Result<usize, Jpeg2000Error> {
        let site = self.site;
        let stride = self.tile_bytes.saturating_add(self.length_bytes);
        let mut position = start;
        for entry in self.entries.chunks_exact(stride) {
            let (tile, length) = entry
                .split_at_checked(self.tile_bytes)
                .ok_or_else(|| site.invalid("TLM entry is incomplete"))?;
            let part = index.sequence().get(position).ok_or_else(|| {
                site.invalid("TLM describes more tile-parts than the codestream has")
            })?;
            if let Some(expected) = read_be(tile)
                && u64::from(part.tile_index()) != expected
            {
                return site.reject_invalid("TLM names a different tile");
            }
            if read_be(length) != Some(u64::try_from(part.length()).unwrap_or(u64::MAX)) {
                return site.reject_invalid("TLM length disagrees with Psot");
            }
            position = position
                .checked_add(1)
                .ok_or_else(|| site.invalid("TLM entry count overflows"))?;
        }
        Ok(position)
    }
}

/// Source marker for packet lengths up to an exclusive index.
#[derive(Clone, Copy, Debug)]
struct LengthSource {
    end: usize,
    site: MarkerSite,
}

/// Packet lengths declared for one tile by PLT segments.
#[derive(Debug, Default)]
pub(crate) struct PacketLengths {
    lengths: Vec<u32>,
    sources: Vec<LengthSource>,
}

impl PacketLengths {
    /// Collects the PLT lists in one tile's tile-part headers.
    ///
    /// # Errors
    ///
    /// Returns `InvalidMarker` for an out-of-order or truncated list, and
    /// `LimitExceeded` when the table passes the caller's bound.
    pub(crate) fn collect(
        index: &TilePartIndex<'_>,
        tile: u32,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        let mut lengths = Vec::new();
        let mut sources = Vec::new();
        let mut sequence = 0u8;
        for part in index.tile_parts(tile) {
            let mut markers = MarkerReader::new_at(part.header(), part.offset());
            while let Some(segment) = markers.next_segment()? {
                if segment.marker() != Marker::Plt {
                    continue;
                }
                let start = lengths.len();
                read_list(segment, &mut sequence, &mut lengths, workspace)?;
                if lengths.len() > start {
                    workspace.push(
                        &mut sources,
                        LengthSource {
                            end: lengths.len(),
                            site: segment.site(),
                        },
                    )?;
                }
            }
        }
        Ok(Self { lengths, sources })
    }

    /// Returns whether the tile declared any packet length.
    pub(crate) fn is_empty(&self) -> bool {
        self.lengths.is_empty()
    }

    /// Checks one packet's observed length against the declared table.
    ///
    /// # Errors
    ///
    /// Returns `InvalidMarker` when the table contradicts the codestream.
    pub(crate) fn verify(&self, packet: usize, observed: usize) -> Result<(), Jpeg2000Error> {
        let Some(expected) = self.lengths.get(packet) else {
            return Ok(());
        };
        if u64::from(*expected) == u64::try_from(observed).unwrap_or(u64::MAX) {
            return Ok(());
        }
        let source = self
            .sources
            .get(self.sources.partition_point(|source| source.end <= packet))
            .ok_or(Jpeg2000Error::Overflow {
                context: "PLT source lookup",
            })?;
        source
            .site
            .reject_invalid("PLT length disagrees with the packet")
    }
}

/// Checks the framing of the main header's PLM segments.
///
/// # Errors
///
/// Returns `InvalidMarker` for an out-of-order sequence index or an `Nplm`
/// group that runs past its segment.
pub(crate) fn verify_main_lengths(main: &MainHeader<'_>) -> Result<(), Jpeg2000Error> {
    let mut markers = main.markers();
    let mut sequence = 0u8;
    while let Some(segment) = markers.next_segment()? {
        if segment.marker() != Marker::Plm {
            continue;
        }
        let site = segment.site();
        let Some((order, mut groups)) = segment.payload().split_first() else {
            return site.reject_invalid("PLM segment is empty");
        };
        if *order != sequence {
            return site.reject_invalid("PLM segments are out of order");
        }
        sequence = sequence.saturating_add(1);
        while let Some((count, rest)) = groups.split_first() {
            let (_, remainder) = rest
                .split_at_checked(usize::from(*count))
                .ok_or_else(|| site.invalid("PLM group runs past its segment"))?;
            groups = remainder;
        }
    }
    Ok(())
}

/// Reads one PLT list of variable-length packet lengths.
fn read_list(
    segment: MarkerSegment<'_>,
    sequence: &mut u8,
    lengths: &mut Vec<u32>,
    workspace: &mut Workspace,
) -> Result<(), Jpeg2000Error> {
    let site = segment.site();
    let Some((order, values)) = segment.payload().split_first() else {
        return site.reject_invalid("PLT segment is empty");
    };
    if *order != *sequence {
        return site.reject_invalid("PLT segments are out of order");
    }
    *sequence = sequence.saturating_add(1);
    let mut value = 0u32;
    let mut pending = false;
    for byte in values {
        value = value
            .checked_shl(VALUE_BITS)
            .map(|shifted| shifted | u32::from(byte & VALUE_MASK))
            .ok_or_else(|| site.invalid("PLT length overflows"))?;
        if byte & CONTINUATION == 0 {
            workspace.push(lengths, value)?;
            value = 0;
            pending = false;
        } else {
            pending = true;
        }
    }
    if pending {
        return site.reject_invalid("PLT length is truncated");
    }
    Ok(())
}

/// Reads a big-endian field of up to eight bytes.
fn read_be(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    Some(bytes.iter().fold(0u64, |value, byte| {
        value.wrapping_shl(u8::BITS) | u64::from(*byte)
    }))
}
