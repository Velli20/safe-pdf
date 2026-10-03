//! Tile-part framing between the main header and the end of the codestream.
//!
//! Every tile-part declares its own length in SOT, so compressed bodies are
//! skipped by that length instead of being scanned for marker-looking bytes.
//! Tile-part header overrides are validated here and re-read by the tile
//! decoder; only per-tile counts are retained.

use crate::{
    Jpeg2000Error, Resource,
    codestream::{MainHeader, Marker, MarkerReader, MarkerSegment},
    coding::{
        CodingParameters, CodingStyle, ComponentCodingStyle, ProgressionChanges, RegionOfInterest,
    },
    limits::DecoderLimits,
    marker_reader::EOC,
    offset_site::OffsetSite,
    quantization::{ComponentQuantization, Quantization},
    workspace::Workspace,
};

/// Bytes in a marker code, which Psot counts from the SOT marker itself.
const MARKER_BYTES: usize = 2;

/// SOT fields describing one tile-part's tile, length, and position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SotHeader {
    tile: u32,
    /// Psot: bytes from the SOT marker to the end of this tile-part.
    length: u32,
    /// TPsot: this tile-part's index within its tile.
    index: u8,
    /// TNsot: tile-parts in this tile, or zero while still unknown.
    parts: u8,
}

impl TryFrom<MarkerSegment<'_>> for SotHeader {
    type Error = Jpeg2000Error;

    fn try_from(segment: MarkerSegment<'_>) -> Result<Self, Self::Error> {
        let [tile_hi, tile_lo, a, b, c, d, index, parts] = segment.payload() else {
            return segment.site().reject_invalid("SOT fields are incomplete");
        };
        Ok(Self {
            tile: u32::from(u16::from_be_bytes([*tile_hi, *tile_lo])),
            length: u32::from_be_bytes([*a, *b, *c, *d]),
            index: *index,
            parts: *parts,
        })
    }
}

/// Tile-part bookkeeping for one tile of the SIZ tile grid.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TileProgress {
    /// Tile-parts seen so far.
    parts: u8,
    /// Tile-parts declared by TNsot, or zero while still unknown.
    expected: u8,
    /// Whether the tile's tile-parts contradicted its TNsot, which is then
    /// disregarded so the tile completes at EOC.
    count_contradicted: bool,
}

impl TileProgress {
    /// Disregards TNsot for this tile from now on.
    ///
    /// Some encoders write a TNsot one short of the tile-parts they emit, and
    /// readers accept such files, so a contradicted count is not an error.
    fn contradict_count(&mut self) {
        self.count_contradicted = true;
        self.expected = 0;
    }
}

/// One tile-part header and compressed body from the codestream.
///
/// Part 1 permits several tile-parts per tile. Both slices borrow the input, so
/// a tile can be reconstructed without copying compressed bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TilePart<'a> {
    tile: u32,
    index: u8,
    expected_parts: Option<u8>,
    site: OffsetSite,
    length: usize,
    header: &'a [u8],
    body: &'a [u8],
}

impl<'a> TilePart<'a> {
    /// Returns the zero-based SIZ tile index from Isot.
    pub fn tile_index(&self) -> u32 {
        self.tile
    }

    /// Returns this tile-part's index within its tile, from TPsot.
    pub fn part_index(&self) -> u8 {
        self.index
    }

    /// Returns the tile-part count from TNsot once the codestream declares it.
    ///
    /// The count is advisory: it is `None` for a tile whose tile-parts
    /// outnumber or disagree with its TNsot.
    pub fn expected_parts(&self) -> Option<u8> {
        self.expected_parts
    }

    /// Returns the byte offset of the SOT marker starting this tile-part.
    pub fn offset(&self) -> usize {
        self.site.offset()
    }

    /// Returns the SOT position for structural diagnostics.
    pub(crate) fn site(&self) -> OffsetSite {
        self.site
    }

    /// Returns the tile-part's length from its SOT marker to its last byte.
    ///
    /// This is the effective Psot, resolved when the field is zero.
    pub fn length(&self) -> usize {
        self.length
    }

    /// Borrows the tile-part marker header between SOT and SOD.
    pub fn header(&self) -> &'a [u8] {
        self.header
    }

    /// Borrows the compressed bytes following SOD for this tile-part.
    pub fn body(&self) -> &'a [u8] {
        self.body
    }
}

/// Sequential framing of the tile-parts that follow a validated main header.
///
/// The reader owns one small progress entry per tile of the SIZ grid, which is
/// charged against [`DecoderLimits::max_working_bytes`] when it is created.
#[derive(Debug)]
pub struct TilePartReader<'a> {
    header: MainHeader<'a>,
    markers: MarkerReader<'a>,
    tiles: Vec<TileProgress>,
    end_of_codestream: bool,
}

impl<'a> TilePartReader<'a> {
    /// Creates a reader positioned at the first tile-part of a codestream.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` if the tile grid or its progress table exceeds
    /// the caller's bounds.
    pub(crate) fn new(
        header: &MainHeader<'a>,
        limits: DecoderLimits,
    ) -> Result<Self, Jpeg2000Error> {
        let tiles = usize::try_from(header.size().limited_tiles(limits)?).map_err(|_| {
            Jpeg2000Error::Overflow {
                context: "SIZ tile count",
            }
        })?;
        let working_bytes = tiles.saturating_mul(size_of::<TileProgress>());
        Resource::WorkingBytes.check_bytes(working_bytes, limits.max_working_bytes)?;
        Ok(Self {
            header: *header,
            markers: MarkerReader::new_at(header.body(), header.body_offset()),
            tiles: vec![TileProgress::default(); tiles],
            end_of_codestream: false,
        })
    }

    /// Returns the number of tiles whose tile-parts have all been read.
    ///
    /// A tile that never declares TNsot is complete only once EOC closes the
    /// codestream, so it counts towards this total from that point on.
    pub fn complete_tiles(&self) -> u32 {
        let complete = self
            .tiles
            .iter()
            .filter(|tile| self.is_complete(tile))
            .count();
        u32::try_from(complete).unwrap_or(u32::MAX)
    }

    /// Returns whether a tile has received all the tile-parts it declares.
    fn is_complete(&self, tile: &TileProgress) -> bool {
        tile.parts > 0
            && match tile.expected {
                0 => self.end_of_codestream,
                expected => tile.parts == expected,
            }
    }

    /// Returns the next framed tile-part, or `None` after the EOC marker.
    ///
    /// # Errors
    ///
    /// Returns a structural error for a misplaced marker, an impossible Psot,
    /// an out-of-order TPsot, or data following the end of the codestream.
    pub fn next_part(&mut self) -> Result<Option<TilePart<'a>>, Jpeg2000Error> {
        if self.end_of_codestream {
            return Ok(None);
        }
        let segment = self.markers.next_required()?;
        let following_site = self.markers.site().marker(segment.code());
        match segment.marker() {
            Marker::Eoc => {
                self.end_of_codestream = true;
                if self.markers.remaining().is_empty() {
                    return Ok(None);
                }
                following_site.reject_invalid("data follows the end of the codestream")
            }
            Marker::Sot => Ok(Some(self.read_part(segment)?)),
            _ => segment.site().out_of_order(),
        }
    }

    /// Checks that every tile was delivered and the codestream was closed.
    ///
    /// Call this after [`TilePartReader::next_part`] returns `None`.
    ///
    /// # Errors
    ///
    /// Returns a structural error if EOC is missing or a tile is incomplete.
    pub fn verify_complete(&self) -> Result<(), Jpeg2000Error> {
        if !self.end_of_codestream {
            return self.markers.site().reject_truncated("end of codestream");
        }
        let incomplete = self.tiles.iter().position(|tile| !self.is_complete(tile));
        match incomplete {
            None => Ok(()),
            Some(index) => Err(Jpeg2000Error::InvalidTilePart {
                offset: self.markers.position(),
                tile: u32::try_from(index).unwrap_or(u32::MAX),
                part: 0,
                reason: "tile is missing one or more tile-parts",
            }),
        }
    }

    /// Frames one tile-part after its SOT segment has been read.
    fn read_part(&mut self, segment: MarkerSegment<'a>) -> Result<TilePart<'a>, Jpeg2000Error> {
        let site = segment.site().offset_site();
        let sot = SotHeader::try_from(segment)?;
        let offset = site.offset();
        let expected_parts = self.record_part(&sot, offset)?;
        let header = self.read_tile_header(&sot, site)?;
        let consumed =
            self.markers
                .position()
                .checked_sub(offset)
                .ok_or(Jpeg2000Error::Overflow {
                    context: "tile-part length",
                })?;
        let body = self.read_body(&sot, offset, consumed)?;
        Ok(TilePart {
            tile: sot.tile,
            index: sot.index,
            expected_parts,
            site,
            length: consumed.saturating_add(body.len()),
            header,
            body,
        })
    }

    /// Records this tile-part against its tile and returns the declared count.
    fn record_part(&mut self, sot: &SotHeader, offset: usize) -> Result<Option<u8>, Jpeg2000Error> {
        let index = usize::try_from(sot.tile).unwrap_or(usize::MAX);
        let invalid = |reason| Jpeg2000Error::InvalidTilePart {
            offset,
            tile: sot.tile,
            part: u16::from(sot.index),
            reason,
        };
        let Some(progress) = self.tiles.get_mut(index) else {
            return Err(invalid("tile index is outside the SIZ tile grid"));
        };
        if sot.index != progress.parts {
            return Err(invalid("tile-part index is out of order"));
        }
        if sot.parts != 0 && !progress.count_contradicted {
            if progress.expected != 0 && progress.expected != sot.parts {
                progress.contradict_count();
            } else {
                progress.expected = sot.parts;
            }
        }
        let parts = progress
            .parts
            .checked_add(1)
            .ok_or_else(|| invalid("too many tile-parts for one tile"))?;
        if progress.expected != 0 && parts > progress.expected {
            progress.contradict_count();
        }
        progress.parts = parts;
        let expected = progress.expected;
        Ok((expected != 0).then_some(expected))
    }

    /// Reads the tile-part header markers up to SOD and validates them.
    fn read_tile_header(
        &mut self,
        sot: &SotHeader,
        site: OffsetSite,
    ) -> Result<&'a [u8], Jpeg2000Error> {
        let start = self.markers.remaining();
        loop {
            let segment = self.markers.next_required()?;
            match segment.marker() {
                Marker::Sod => break,
                Marker::Cod | Marker::Qcd if sot.index != 0 => {
                    return segment.site().out_of_order();
                }
                Marker::Cod
                | Marker::Coc
                | Marker::Qcd
                | Marker::Qcc
                | Marker::Rgn
                | Marker::Poc
                | Marker::Plt
                | Marker::Ppt
                | Marker::Com => {}
                _ => {
                    return segment.site().out_of_order();
                }
            }
        }
        let consumed = start
            .len()
            .saturating_sub(self.markers.remaining().len())
            .saturating_sub(MARKER_BYTES);
        let header = start
            .get(..consumed)
            .ok_or_else(|| site.truncated("tile-part header"))?;
        self.validate_tile_header(header, site.offset())?;
        Ok(header)
    }

    /// Cross-checks a tile-part header's overrides against the tile defaults.
    ///
    /// The header is re-read once it is framed, so a quantization override can
    /// be checked against a coding override that appears after it.
    fn validate_tile_header(&self, header: &'a [u8], offset: usize) -> Result<(), Jpeg2000Error> {
        let components = self.header.size().component_count();
        let mut tile_coding = None;
        let mut markers = MarkerReader::new_at(header, offset);
        while let Some(segment) = markers.next_segment()? {
            if segment.marker() == Marker::Cod {
                tile_coding = Some(CodingStyle::try_from(segment)?);
            }
        }
        let coding = tile_coding.unwrap_or_else(|| *self.header.coding());
        let mut markers = MarkerReader::new_at(header, offset);
        while let Some(segment) = markers.next_segment()? {
            match segment.marker() {
                Marker::Coc => {
                    ComponentCodingStyle::parse(segment, components)?;
                }
                Marker::Qcd => {
                    Quantization::parse(segment)?.validate_subbands(&coding.parameters)?;
                }
                Marker::Qcc => {
                    let override_ = ComponentQuantization::parse(segment, components)?;
                    let parameters = self.component_parameters(
                        header,
                        offset,
                        override_.component,
                        tile_coding.as_ref(),
                    )?;
                    override_.quantization.validate_subbands(&parameters)?;
                }
                Marker::Rgn => {
                    RegionOfInterest::parse(segment, components)?;
                }
                Marker::Poc => {
                    ProgressionChanges::parse(segment, components, &coding)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Returns the coding parameters applying to one component of this tile.
    fn component_parameters(
        &self,
        header: &'a [u8],
        offset: usize,
        component: u16,
        tile_coding: Option<&CodingStyle<'a>>,
    ) -> Result<CodingParameters<'a>, Jpeg2000Error> {
        let components = self.header.size().component_count();
        let tile_override = ComponentCodingStyle::find(
            MarkerReader::new_at(header, offset),
            component,
            components,
        )?;
        match (tile_override, tile_coding) {
            // Precedence runs from the tile's component override, through the
            // tile default, to the main header's override or default.
            (Some(parameters), _) => Ok(parameters),
            (None, Some(coding)) => Ok(coding.parameters),
            (None, None) => self.header.component_parameters(component),
        }
    }

    /// Consumes this tile-part's compressed body using its declared length.
    fn read_body(
        &mut self,
        sot: &SotHeader,
        offset: usize,
        consumed: usize,
    ) -> Result<&'a [u8], Jpeg2000Error> {
        let invalid = |reason| Jpeg2000Error::InvalidTilePart {
            offset,
            tile: sot.tile,
            part: u16::from(sot.index),
            reason,
        };
        let body_len = match sot.length {
            // Psot may be zero in the last tile-part, which then runs to EOC.
            0 => {
                let rest = self.markers.remaining();
                if !rest.ends_with(&EOC.to_be_bytes()) {
                    return Err(invalid("final tile-part is not closed by EOC"));
                }
                rest.len().saturating_sub(MARKER_BYTES)
            }
            length => usize::try_from(length)
                .ok()
                .and_then(|length| length.checked_sub(consumed))
                .ok_or_else(|| invalid("Psot is shorter than the tile-part header"))?,
        };
        self.markers.take_bytes(body_len)
    }
}

/// Every tile-part of a codestream, grouped by the tile it belongs to.
///
/// Tile-parts of one tile may be interleaved with other tiles, so the parts
/// are framed once and indexed by tile. Only the borrowed header and body
/// slices and two small index tables are retained, all charged against the
/// caller's working-memory bound.
#[derive(Debug)]
pub(crate) struct TilePartIndex<'a> {
    parts: Vec<TilePart<'a>>,
    offsets: Vec<u32>,
    ordered: Vec<u32>,
}

impl<'a> TilePartIndex<'a> {
    /// Frames and indexes every tile-part following a validated main header.
    ///
    /// # Errors
    ///
    /// Returns a structural error for malformed tile-part framing, a missing
    /// tile-part, or a missing EOC, and `LimitExceeded` when the index passes
    /// the caller's working-memory bound.
    pub(crate) fn build(
        header: &MainHeader<'a>,
        limits: DecoderLimits,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        let mut reader = TilePartReader::new(header, limits)?;
        let mut parts = Vec::new();
        while let Some(part) = reader.next_part()? {
            workspace.push(&mut parts, part)?;
        }
        reader.verify_complete()?;
        let tiles = usize::try_from(header.size().limited_tiles(limits)?).map_err(|_| {
            Jpeg2000Error::Overflow {
                context: "SIZ tile count",
            }
        })?;
        Self::index(parts, tiles, workspace)
    }

    /// Groups framed tile-parts by tile while preserving their TPsot order.
    fn index(
        parts: Vec<TilePart<'a>>,
        tiles: usize,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "tile-part index",
        };
        let mut offsets: Vec<u32> = workspace.vector(tiles.checked_add(1).ok_or_else(overflow)?)?;
        for part in &parts {
            let slot = offsets
                .get_mut(usize::try_from(part.tile).map_err(|_| overflow())?)
                .ok_or_else(overflow)?;
            *slot = slot.checked_add(1).ok_or_else(overflow)?;
        }
        let mut running = 0u32;
        for slot in &mut offsets {
            let count = *slot;
            *slot = running;
            running = running.checked_add(count).ok_or_else(overflow)?;
        }
        let mut cursors = offsets.clone();
        let mut ordered: Vec<u32> = workspace.vector(parts.len())?;
        for (position, part) in parts.iter().enumerate() {
            let tile = usize::try_from(part.tile).map_err(|_| overflow())?;
            let cursor = cursors.get_mut(tile).ok_or_else(overflow)?;
            let slot = ordered
                .get_mut(usize::try_from(*cursor).map_err(|_| overflow())?)
                .ok_or_else(overflow)?;
            *slot = u32::try_from(position).map_err(|_| overflow())?;
            *cursor = cursor.checked_add(1).ok_or_else(overflow)?;
        }
        Ok(Self {
            parts,
            offsets,
            ordered,
        })
    }

    /// Returns every tile-part in codestream order.
    ///
    /// PPM associates its packet-header groups with this order.
    pub(crate) fn sequence(&self) -> &[TilePart<'a>] {
        &self.parts
    }

    /// Returns the tile-parts of one tile in TPsot order.
    pub(crate) fn tile_parts(&self, tile: u32) -> impl Iterator<Item = &TilePart<'a>> {
        let range = self.range(tile);
        self.ordered
            .get(range)
            .unwrap_or_default()
            .iter()
            .filter_map(|position| self.parts.get(usize::try_from(*position).ok()?))
    }

    /// Returns the slice of the grouping table belonging to one tile.
    fn range(&self, tile: u32) -> core::ops::Range<usize> {
        let index = usize::try_from(tile).unwrap_or(usize::MAX);
        let start = index
            .checked_add(1)
            .and_then(|next| Some((*self.offsets.get(index)?, *self.offsets.get(next)?)));
        match start {
            Some((first, last)) => {
                let first = usize::try_from(first).unwrap_or(usize::MAX);
                let last = usize::try_from(last).unwrap_or(usize::MAX);
                first..last.max(first)
            }
            None => 0..0,
        }
    }
}
