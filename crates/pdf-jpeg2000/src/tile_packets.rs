//! Reading one tile's packets into per-code-block codeword segments.
//!
//! A tile's compressed data is a flat sequence of packets whose order Annex
//! B.12 fixes. Each packet names the code-blocks of one precinct that
//! contribute to one quality layer, and carries their bytes immediately after
//! its header unless PPM or PPT moved the header elsewhere.
//!
//! The result is the input to Tier-1: for every code-block, how many bit-planes
//! are missing, how many coding passes arrived, and the borrowed codeword
//! segments carrying them. A code-block collects segments across several
//! layers, so its segments form a linked chain through one shared table rather
//! than a contiguous run.

use pdf_graphics::Size;

use crate::{
    Jpeg2000Error,
    chunked_bytes::{ChunkCursor, ChunkedBytes},
    codestream::MainHeader,
    offset_site::OffsetSite,
    packed_headers::PackedHeaders,
    packet_header::{PacketContributions, PacketSegment, PrecinctBand},
    packet_lengths::PacketLengths,
    progression::{ComponentProgression, PacketLocator, ProgressionPlan, ResolutionProgression},
    stuffed_bits::{HeaderSource, StuffedBitReader},
    tile_coding::TileCoding,
    tile_part::TilePartIndex,
    tile_structure::TileStructure,
    workspace::Workspace,
};

/// Bytes in a start-of-packet marker segment, including its marker code.
const SOP_BYTES: usize = 6;
/// Start-of-packet marker code.
const SOP: [u8; 2] = [0xff, 0x91];
/// End-of-packet-header marker code.
const EPH: [u8; 2] = [0xff, 0x92];

/// One codeword segment of a code-block, borrowed from the tile-part body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CodewordSegment<'a> {
    /// Coding passes the segment carries.
    pub(crate) passes: u32,
    /// Compressed bytes of the segment.
    pub(crate) data: &'a [u8],
    /// Next segment of the same code-block, in coding-pass order.
    next: Option<u32>,
}

/// What Tier-2 learned about one code-block.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BlockState {
    /// Most significant bit-planes known to hold no coefficient.
    pub(crate) zero_bit_planes: u32,
    /// Coding passes the block received across every layer.
    pub(crate) passes: u32,
    first_segment: Option<u32>,
    last_segment: Option<u32>,
}

/// Every code-block of one tile, after its packets have been read.
#[derive(Debug)]
pub(crate) struct TileBlocks<'a> {
    states: Vec<BlockState>,
    segments: Vec<CodewordSegment<'a>>,
}

impl<'a> TileBlocks<'a> {
    /// Reads every packet of one tile.
    ///
    /// # Errors
    ///
    /// Returns a structural error for a truncated or inconsistent packet
    /// sequence, and `LimitExceeded` when the tile's tables pass the caller's
    /// working-memory bound.
    pub(crate) fn read(
        structure: &TileStructure<'a>,
        coding: &TileCoding<'a, '_>,
        main: &MainHeader<'a>,
        index: &TilePartIndex<'a>,
        tile: u32,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        let body = Self::tile_body(index, tile, workspace)?;
        let packed = PackedHeaders::collect(main, index, tile, workspace)?;
        let lengths = PacketLengths::collect(index, tile, workspace)?;
        let locators = Self::packet_order(structure, coding, workspace)?;
        let mut precincts = Vec::new();
        for slot in 0..structure.slot_count() {
            let layout = structure.slot(slot).ok_or(Jpeg2000Error::Overflow {
                context: "precinct slot index",
            })?;
            let state = PrecinctBand::new(layout.grid, workspace)?;
            workspace.push(&mut precincts, state)?;
        }
        let mut reader = PacketStream {
            structure,
            body: &body,
            packed: packed.stream(),
            lengths: &lengths,
            style: coding.style().parameters.block_style,
            sop: coding.style().sop,
            eph: coding.style().eph,
            site: index
                .tile_parts(tile)
                .next()
                .map_or_else(OffsetSite::default, |part| part.site()),
            body_position: 0,
            header_position: 0,
            contributions: PacketContributions::default(),
        };
        let mut blocks = Self {
            states: workspace.vector(structure.block_count())?,
            segments: Vec::new(),
        };
        for (packet, locator) in locators.iter().enumerate() {
            reader.read_packet(packet, locator, &mut precincts, &mut blocks, workspace)?;
        }
        reader.finish()?;
        Ok(blocks)
    }

    /// Returns the Tier-2 result for one code-block of the tile.
    pub(crate) fn state(&self, block: usize) -> Option<&BlockState> {
        self.states.get(block)
    }

    /// Returns one code-block's codeword segments in coding-pass order.
    pub(crate) fn segments(&self, state: &BlockState) -> impl Iterator<Item = CodewordSegment<'a>> {
        let mut next = state.first_segment;
        core::iter::from_fn(move || {
            let segment = *self.segments.get(usize::try_from(next?).ok()?)?;
            next = segment.next;
            Some(segment)
        })
    }

    /// Appends one codeword segment to a code-block's chain.
    fn append(
        &mut self,
        block: usize,
        zero_bit_planes: u32,
        segment: CodewordSegment<'a>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "codeword segment table",
        };
        let position = u32::try_from(self.segments.len()).map_err(|_| overflow())?;
        workspace.push(&mut self.segments, segment)?;
        let state = self.states.get_mut(block).ok_or_else(overflow)?;
        let previous = state.last_segment;
        state.zero_bit_planes = zero_bit_planes;
        state.last_segment = Some(position);
        state.first_segment = state.first_segment.or(Some(position));
        state.passes = state.passes.saturating_add(segment.passes);
        if let Some(previous) = previous {
            let earlier = self
                .segments
                .get_mut(usize::try_from(previous).map_err(|_| overflow())?)
                .ok_or_else(overflow)?;
            earlier.next = Some(position);
        }
        Ok(())
    }

    /// Concatenates the compressed bodies of one tile's tile-parts.
    fn tile_body(
        index: &TilePartIndex<'a>,
        tile: u32,
        workspace: &mut Workspace,
    ) -> Result<ChunkedBytes<'a>, Jpeg2000Error> {
        let mut body = ChunkedBytes::default();
        for part in index.tile_parts(tile) {
            body.push(part.body(), workspace)?;
        }
        Ok(body)
    }

    /// Builds the tile's packet sequence from its progression steps.
    fn packet_order(
        structure: &TileStructure<'a>,
        coding: &TileCoding<'a, '_>,
        workspace: &mut Workspace,
    ) -> Result<Vec<PacketLocator>, Jpeg2000Error> {
        let mut components = Vec::new();
        for entry in structure.components() {
            let mut resolutions = Vec::new();
            for level in &entry.resolutions {
                let record = ResolutionProgression {
                    region: level.resolution.region(),
                    exponents: level.precincts.exponents(),
                    columns: level.precincts.columns(),
                    rows: level.precincts.rows(),
                };
                workspace.push(&mut resolutions, record)?;
            }
            let record = ComponentProgression {
                subsampling: Size {
                    width: entry.component.info.x_subsampling,
                    height: entry.component.info.y_subsampling,
                },
                levels: entry.parameters.levels,
                resolutions,
            };
            workspace.push(&mut components, record)?;
        }
        let resolutions = components
            .iter()
            .map(ComponentProgression::resolution_count)
            .max()
            .unwrap_or(0);
        let component_count = u16::try_from(components.len()).unwrap_or(u16::MAX);
        let mut steps = Vec::new();
        coding.progression_steps(
            0..component_count,
            0..resolutions,
            0..coding.style().layers,
            &mut steps,
            workspace,
        )?;
        let mut locators = Vec::new();
        ProgressionPlan::new(structure.tile_region(), &components).build(
            &steps,
            &mut locators,
            workspace,
        )?;
        Ok(locators)
    }
}

/// Cursor state shared by every packet of one tile.
struct PacketStream<'a, 'state> {
    structure: &'state TileStructure<'a>,
    body: &'state ChunkedBytes<'a>,
    packed: Option<&'state ChunkedBytes<'a>>,
    lengths: &'state PacketLengths,
    style: crate::coding::CodeBlockFlags,
    sop: bool,
    eph: bool,
    site: OffsetSite,
    body_position: usize,
    header_position: usize,
    contributions: PacketContributions,
}

impl<'a> PacketStream<'a, '_> {
    /// Reads one packet's header and body.
    fn read_packet(
        &mut self,
        packet: usize,
        locator: &PacketLocator,
        precincts: &mut [PrecinctBand],
        blocks: &mut TileBlocks<'a>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        let Some(level) = self.structure.level(locator.component, locator.resolution) else {
            return Ok(());
        };
        let Some(range) = level.precinct_slots(locator.precinct) else {
            return Ok(());
        };
        let start = self.body_position;
        self.skip_start_of_packet()?;
        let bands = precincts
            .get_mut(range.clone())
            .ok_or(Jpeg2000Error::Overflow {
                context: "precinct slot range",
            })?;
        self.read_header(locator.layer, bands)?;
        self.read_bodies(range.start, blocks, workspace)?;
        if self.packed.is_some() || self.lengths.is_empty() {
            // A PLT length counts the whole packet, so it can only be
            // compared when the header sits in the packet rather than in a
            // PPM or PPT segment.
            return Ok(());
        }
        self.lengths
            .verify(packet, self.body_position.saturating_sub(start))
    }

    /// Reads a packet header from the packet or from the packed stream.
    fn read_header(&mut self, layer: u16, bands: &mut [PrecinctBand]) -> Result<(), Jpeg2000Error> {
        let (stream, position) = match self.packed {
            Some(packed) => (packed, self.header_position),
            None => (self.body, self.body_position),
        };
        let cursor = stream
            .cursor_at(position)
            .ok_or_else(|| self.site.truncated("packet header"))?;
        let mut reader = StuffedBitReader::new(cursor, self.site.offset());
        self.contributions
            .read(&mut reader, layer, self.style, bands)?;
        let consumed = self.check_end_of_header(&mut reader)?;
        match self.packed {
            Some(_) => self.header_position = self.header_position.saturating_add(consumed),
            None => self.body_position = self.body_position.saturating_add(consumed),
        }
        Ok(())
    }

    /// Consumes the optional end-of-packet-header marker.
    fn check_end_of_header(
        &self,
        reader: &mut StuffedBitReader<ChunkCursor<'a, '_>>,
    ) -> Result<usize, Jpeg2000Error> {
        if !self.eph {
            return Ok(reader.consumed());
        }
        let mut marker = [0u8; EPH.len()];
        for slot in &mut marker {
            *slot = reader
                .source_mut()
                .next_byte()
                .ok_or_else(|| self.site.truncated("end of packet header"))?;
        }
        let marker_site = self.site.marker(u16::from_be_bytes(marker));
        if marker != EPH {
            return marker_site.reject_invalid("expected an end-of-packet-header marker");
        }
        Ok(reader.consumed())
    }

    /// Consumes the optional start-of-packet marker segment.
    fn skip_start_of_packet(&mut self) -> Result<(), Jpeg2000Error> {
        if !self.sop {
            return Ok(());
        }
        let Some(marker) = self.body.contiguous(self.body_position, SOP.len()) else {
            return Ok(());
        };
        if marker != SOP {
            return Ok(());
        }
        if self
            .body
            .contiguous(self.body_position, SOP_BYTES)
            .is_none()
        {
            return self.site.reject_truncated("start of packet marker");
        }
        self.body_position = self.body_position.saturating_add(SOP_BYTES);
        Ok(())
    }

    /// Slices the packet body into the codeword segments its header named.
    fn read_bodies(
        &mut self,
        first_slot: usize,
        blocks: &mut TileBlocks<'a>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        for segment in self.contributions.segments() {
            let block = self.block_index(first_slot, segment)?;
            let data = self
                .body
                .contiguous(self.body_position, segment.length)
                .ok_or_else(|| self.site.truncated("packet body"))?;
            self.body_position =
                self.body_position
                    .checked_add(segment.length)
                    .ok_or(Jpeg2000Error::Overflow {
                        context: "packet body length",
                    })?;
            blocks.append(
                block,
                segment.zero_bit_planes,
                CodewordSegment {
                    passes: segment.passes,
                    data,
                    next: None,
                },
                workspace,
            )?;
        }
        Ok(())
    }

    /// Maps a header entry to its code-block in the tile's block table.
    fn block_index(
        &self,
        first_slot: usize,
        segment: &PacketSegment,
    ) -> Result<usize, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "code-block index",
        };
        let slot = first_slot.checked_add(segment.band).ok_or_else(overflow)?;
        let layout = self.structure.slot(slot).ok_or_else(overflow)?;
        if segment.block >= layout.block_count() {
            return Err(overflow());
        }
        usize::try_from(layout.first_block)
            .ok()
            .and_then(|first| first.checked_add(usize::try_from(segment.block).ok()?))
            .ok_or_else(overflow)
    }

    /// Checks that the packets consumed the tile's compressed data exactly.
    fn finish(&self) -> Result<(), Jpeg2000Error> {
        if self.body_position == self.body.len() {
            return Ok(());
        }
        self.site.reject_truncated("tile packet data")
    }
}
