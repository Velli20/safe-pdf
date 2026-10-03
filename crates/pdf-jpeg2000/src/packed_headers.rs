//! Packet headers carried outside their packets, by PPM or PPT.
//!
//! Annex A.7.4 and A.7.5 let an encoder move every packet header out of the
//! packet bodies: PPM collects them in the main header, PPT in a tile-part
//! header. In both cases the bodies keep their place in the tile-part data and
//! only the headers move, so a tile decoder reads headers from one stream and
//! bodies from another.
//!
//! PPM concatenates the payloads of all its segments into one stream of
//! length-prefixed groups, one group per tile-part in codestream order, as
//! `Nppm` is defined. PPT concatenates the payloads of the segments inside one
//! tile-part header. A codestream that uses both is rejected: Annex A.7.4
//! gives PPM authority over every tile-part, leaving no meaning for a PPT
//! segment beside it.

use crate::{
    Jpeg2000Error,
    chunked_bytes::ChunkedBytes,
    codestream::{MainHeader, Marker, MarkerReader},
    tile_part::TilePartIndex,
    workspace::Workspace,
};

/// Bytes of the `Zppm` or `Zppt` sequence index opening each payload.
const SEQUENCE_BYTES: usize = 1;

/// The packet-header stream of one tile, when the codestream packs headers.
#[derive(Debug, Default)]
pub(crate) struct PackedHeaders<'a> {
    stream: Option<ChunkedBytes<'a>>,
}

impl<'a> PackedHeaders<'a> {
    /// Collects the packed packet headers that apply to one tile.
    ///
    /// Returns an empty result when the codestream carries packet headers in
    /// their packets, which is the common case.
    ///
    /// # Errors
    ///
    /// Returns a structural error for an out-of-order sequence index, a
    /// truncated PPM group, or a codestream mixing PPM with PPT.
    pub(crate) fn collect(
        main: &MainHeader<'a>,
        index: &TilePartIndex<'a>,
        tile: u32,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        let packed = Self::main_stream(main, workspace)?;
        if packed.is_empty() {
            return Ok(Self {
                stream: Self::tile_stream(index, tile, workspace)?,
            });
        }
        if Self::tile_stream(index, tile, workspace)?.is_some() {
            return Err(Jpeg2000Error::UnsupportedFeature {
                feature: "PPT segments beside a PPM main header",
            });
        }
        Ok(Self {
            stream: Some(Self::split_main_stream(&packed, index, tile, workspace)?),
        })
    }

    /// Returns the tile's packed header stream, if the codestream packs them.
    pub(crate) fn stream(&self) -> Option<&ChunkedBytes<'a>> {
        self.stream.as_ref()
    }

    /// Concatenates every PPM payload of the main header.
    fn main_stream(
        main: &MainHeader<'a>,
        workspace: &mut Workspace,
    ) -> Result<ChunkedBytes<'a>, Jpeg2000Error> {
        Self::concatenate(main.markers(), Marker::Ppm, workspace)
    }

    /// Concatenates every PPT payload in one tile's tile-part headers.
    fn tile_stream(
        index: &TilePartIndex<'a>,
        tile: u32,
        workspace: &mut Workspace,
    ) -> Result<Option<ChunkedBytes<'a>>, Jpeg2000Error> {
        let mut stream = ChunkedBytes::default();
        let mut sequence = 0u32;
        for part in index.tile_parts(tile) {
            let markers = MarkerReader::new_at(part.header(), part.offset());
            sequence = Self::append(markers, Marker::Ppt, sequence, &mut stream, workspace)?;
        }
        Ok((!stream.is_empty()).then_some(stream))
    }

    /// Concatenates the payloads of one packed-header marker in a header.
    fn concatenate(
        markers: MarkerReader<'a>,
        marker: Marker,
        workspace: &mut Workspace,
    ) -> Result<ChunkedBytes<'a>, Jpeg2000Error> {
        let mut stream = ChunkedBytes::default();
        Self::append(markers, marker, 0, &mut stream, workspace)?;
        Ok(stream)
    }

    /// Appends matching payloads, checking their sequence indices.
    fn append(
        mut markers: MarkerReader<'a>,
        marker: Marker,
        mut sequence: u32,
        stream: &mut ChunkedBytes<'a>,
        workspace: &mut Workspace,
    ) -> Result<u32, Jpeg2000Error> {
        while let Some(segment) = markers.next_segment()? {
            if segment.marker() != marker {
                continue;
            }
            let (order, payload) = segment
                .payload()
                .split_at_checked(SEQUENCE_BYTES)
                .ok_or_else(|| {
                    segment
                        .site()
                        .invalid("packed packet header segment is empty")
                })?;
            if order.first().copied().map(u32::from) != Some(sequence & u32::from(u8::MAX)) {
                return segment
                    .site()
                    .reject_invalid("packed packet header segments are out of order");
            }
            sequence = sequence.saturating_add(1);
            stream.push(payload, workspace)?;
        }
        Ok(sequence)
    }

    /// Selects the PPM groups belonging to one tile.
    ///
    /// The stream holds one `Nppm`-prefixed group per tile-part in codestream
    /// order, so the groups of a tile are gathered by walking that order.
    fn split_main_stream(
        packed: &ChunkedBytes<'a>,
        index: &TilePartIndex<'a>,
        tile: u32,
        workspace: &mut Workspace,
    ) -> Result<ChunkedBytes<'a>, Jpeg2000Error> {
        let mut cursor = packed.cursor();
        let mut stream = ChunkedBytes::default();
        for part in index.sequence() {
            let site = part.site();
            let length = usize::try_from(cursor.read_length(site)?).map_err(|_| {
                Jpeg2000Error::Overflow {
                    context: "packed packet header length",
                }
            })?;
            let start = cursor.position();
            cursor.skip(length, site)?;
            if part.tile_index() != tile {
                continue;
            }
            let group = packed.slice(start, length, workspace)?;
            Self::extend(&mut stream, &group, workspace)?;
        }
        Ok(stream)
    }

    /// Appends one PPM group to the tile's header stream.
    fn extend(
        stream: &mut ChunkedBytes<'a>,
        group: &ChunkedBytes<'a>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        for chunk in group.chunks() {
            stream.push(chunk, workspace)?;
        }
        Ok(())
    }
}
