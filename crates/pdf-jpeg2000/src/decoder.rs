//! Typestate-controlled progression from compressed bytes to completed tiles.
//!
//! The state parameter permits only the next Part 1 operation: header parse,
//! tile reconstruction, or completion. Parsed state borrows input bytes, and
//! tile output is scoped to a callback.

use crate::{
    Jpeg2000Error, Resource, TileSink,
    codestream::{ImageHeader, MainHeader, TilePartReader},
    coefficient::{Coefficient, fits_in_32_bits},
    container::InputFormat,
    jp2::ParsedContainer,
    lifting::Lifting,
    limits::DecoderOptions,
    marker_reader::SOC,
    packet_lengths::{TilePartLengths, verify_main_lengths},
    tile::{ComponentPlane, TileBounds, TileView},
    tile_coding::TileCoding,
    tile_layout::TileLayout,
    tile_packets::TileBlocks,
    tile_part::TilePartIndex,
    tile_reconstruct::{self, ComponentOutput},
    tile_structure::TileStructure,
    workspace::Workspace,
};

/// Bytes of one real-valued coefficient, the widest a working plane holds.
const REAL_SAMPLE_BYTES: u64 = 8;

/// The validated header, framed tile-parts, and counters of a started decode.
///
/// Tile-parts are framed once while the header is parsed, so a tile's parts
/// are reachable together however the codestream interleaves them, and the
/// working-memory budget is carried alongside them.
#[derive(Debug)]
pub struct Progress<'a> {
    header: ImageHeader<'a>,
    parts: TilePartIndex<'a>,
    workspace: Workspace,
    discarded_levels: u8,
    decoded_tiles: u32,
    remaining_tiles: u32,
}

impl<'a> Progress<'a> {
    /// Reconstructs the next tile in SIZ order and delivers it to a sink.
    ///
    /// The counters advance only after the sink accepts the tile, and the
    /// tile's scratch storage is returned to the budget either way.
    fn decode_tile(&mut self, sink: &mut dyn TileSink) -> Result<(), Jpeg2000Error> {
        if self.remaining_tiles == 0 {
            return Err(Jpeg2000Error::InvalidTilePart {
                offset: self.header.main().body_offset(),
                tile: self.decoded_tiles,
                part: 0,
                reason: "every tile has already been decoded",
            });
        }
        let tile = self.decoded_tiles;
        let mark = self.workspace.mark();
        let result = self.reconstruct(tile, sink);
        self.workspace.release(mark);
        result?;
        self.decoded_tiles = self.decoded_tiles.saturating_add(1);
        self.remaining_tiles = self.remaining_tiles.saturating_sub(1);
        Ok(())
    }

    /// Runs the decoding stages that turn one tile's packets into samples.
    fn reconstruct(&mut self, tile: u32, sink: &mut dyn TileSink) -> Result<(), Jpeg2000Error> {
        let Self {
            header,
            parts,
            workspace,
            discarded_levels,
            ..
        } = self;
        let main = header.main();
        let coding = TileCoding::resolve(main, parts, tile)?;
        let layout = TileLayout::new(main.size(), tile)?;
        let structure = TileStructure::build(&layout, &coding, workspace)?;
        let blocks = TileBlocks::read(&structure, &coding, main, parts, tile, workspace)?;
        let bounds = TileBounds::from_region(layout.region());
        if fits_in_32_bits(tile_reconstruct::magnitude_bits(&structure)?) {
            let planes = tile_reconstruct::reconstruct::<i32>(
                &structure,
                &blocks,
                &coding,
                *discarded_levels,
                workspace,
            )?;
            deliver(tile, bounds, &planes, sink)
        } else {
            let planes = tile_reconstruct::reconstruct::<i64>(
                &structure,
                &blocks,
                &coding,
                *discarded_levels,
                workspace,
            )?;
            deliver(tile, bounds, &planes, sink)
        }
    }
}

/// Hands one reconstructed tile to the caller's sink.
///
/// The plane views borrow the decoder's sample buffers, so they expire when
/// the sink returns, as [`TileSink`] documents.
fn deliver<C: Coefficient + Lifting>(
    tile: u32,
    bounds: TileBounds,
    planes: &[ComponentOutput<C>],
    sink: &mut dyn TileSink,
) -> Result<(), Jpeg2000Error> {
    let mut views = Vec::new();
    views
        .try_reserve_exact(planes.len())
        .map_err(|_| Jpeg2000Error::Overflow {
            context: "tile component views",
        })?;
    for plane in planes {
        let stride =
            usize::try_from(plane.region.size().width).map_err(|_| Jpeg2000Error::Overflow {
                context: "component plane stride",
            })?;
        views.push(ComponentPlane::new(
            plane.index,
            TileBounds::from_region(plane.region),
            stride,
            plane.info.precision,
            plane.info.signed,
            plane.discarded_levels,
            C::sample_data(&plane.samples),
        ));
    }
    sink.write_tile(TileView::new(tile, bounds, &views))
}

mod sealed {
    /// Prevents outside code from adding decoder states.
    pub trait Sealed {}
}

/// A decoder state holding a validated image header.
pub trait DecodeState<'a>: sealed::Sealed {
    /// Returns the header and tile counters shared by all started states.
    #[doc(hidden)]
    fn progress(&self) -> &Progress<'a>;
}

/// State before a JP2 or codestream header has been parsed.
///
/// Only header parsing is available from this state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Uninitialized;

/// State after Part 1 main-header validation and before the first tile.
///
/// Its private fields prevent callers from forging a parsed SIZ/COD/QCD header
/// or bypassing the decoder's resource checks.
#[derive(Debug)]
pub struct HeaderParsed<'a> {
    progress: Progress<'a>,
}

/// State after at least one tile has been delivered to a [`TileSink`].
///
/// More tiles may remain. The decoder must verify all tiles and EOC before
/// entering [`Complete`].
#[derive(Debug)]
pub struct TileDecoded<'a> {
    progress: Progress<'a>,
}

/// State after every tile and the Part 1 end-of-codestream marker are checked.
#[derive(Debug)]
pub struct Complete<'a> {
    progress: Progress<'a>,
}

macro_rules! decode_state {
    ($state:ident) => {
        impl sealed::Sealed for $state<'_> {}

        impl<'a> DecodeState<'a> for $state<'a> {
            fn progress(&self) -> &Progress<'a> {
                &self.progress
            }
        }
    };
}

decode_state!(HeaderParsed);
decode_state!(TileDecoded);
decode_state!(Complete);

/// Borrowing JPEG 2000 decoder parameterized by its legal next operation.
///
/// The decoder owns only small state and temporary tile workspaces; compressed
/// bytes stay in the caller's slice for lifetime `'a`. State-specific methods
/// prevent completing or emitting tiles before the required Part 1 headers.
#[derive(Debug)]
pub struct Decoder<'a, State> {
    input: &'a [u8],
    options: DecoderOptions,
    state: State,
}

impl<'a> Decoder<'a, Uninitialized> {
    /// Borrows compressed bytes with an explicit format and resource policy.
    ///
    /// Construction does not inspect the input.
    pub fn new(input: &'a [u8], options: DecoderOptions) -> Self {
        Self {
            input,
            options,
            state: Uninitialized,
        }
    }

    /// Parses a raw codestream or JP2 container and validates its main header.
    ///
    /// The successful state borrows SIZ, COD, QCD, and optional JP2 box
    /// payloads from `input` and enforces [`crate::DecoderLimits`] before tile
    /// allocation.
    ///
    /// # Errors
    ///
    /// Returns a structural or resource error for invalid header input.
    pub fn parse_header(self) -> Result<Decoder<'a, HeaderParsed<'a>>, Jpeg2000Error> {
        Resource::InputBytes.check_bytes(self.input.len(), self.options.limits.max_input_bytes)?;
        let header = match self.options.format {
            InputFormat::Codestream => self.raw_header()?,
            InputFormat::Auto if self.input.starts_with(&SOC.to_be_bytes()) => self.raw_header()?,
            requested => self.container_header(requested)?,
        };
        let limits = self.options.limits;
        let remaining_tiles = header.main().size().limited_tiles(limits)?;
        let mut workspace = Workspace::new(limits);
        let parts = TilePartIndex::build(header.main(), limits, &mut workspace)?;
        TilePartLengths::verify(header.main(), &parts)?;
        verify_main_lengths(header.main())?;
        Ok(Decoder {
            input: self.input,
            options: self.options,
            state: HeaderParsed {
                progress: Progress {
                    header,
                    parts,
                    workspace,
                    discarded_levels: 0,
                    decoded_tiles: 0,
                    remaining_tiles,
                },
            },
        })
    }

    /// Parses a bare Part 1 codestream.
    fn raw_header(&self) -> Result<ImageHeader<'a>, Jpeg2000Error> {
        Ok(ImageHeader {
            main: MainHeader::parse(self.input, 0)?,
            format: InputFormat::Codestream,
            jp2: None,
        })
    }

    /// Parses a JP2-family container and compares its image header with SIZ.
    fn container_header(&self, requested: InputFormat) -> Result<ImageHeader<'a>, Jpeg2000Error> {
        let parsed = ParsedContainer::parse(self.input, requested)?;
        let main = MainHeader::parse(parsed.metadata.codestream(), parsed.codestream_offset)?;
        parsed.validate_size(main.size())?;
        Ok(ImageHeader {
            main,
            format: parsed.format,
            jp2: Some(parsed.metadata),
        })
    }
}

impl<'a, State> Decoder<'a, State> {
    /// Returns the borrowed compressed bytes for this decoder instance.
    pub fn input(&self) -> &'a [u8] {
        self.input
    }

    /// Returns the owned format selection and resource policy.
    pub fn options(&self) -> DecoderOptions {
        self.options
    }
}

impl<'a, State: DecodeState<'a>> Decoder<'a, State> {
    /// Returns the borrowed Part 1 main header and optional JP2 metadata.
    pub fn header(&self) -> &ImageHeader<'a> {
        &self.state.progress().header
    }

    /// Returns the number of tiles already emitted to a sink.
    pub fn decoded_tiles(&self) -> u32 {
        self.state.progress().decoded_tiles
    }

    /// Returns the number of SIZ tiles awaiting reconstruction.
    pub fn remaining_tiles(&self) -> u32 {
        self.state.progress().remaining_tiles
    }

    /// Frames the tile-parts that follow the validated main header.
    ///
    /// The reader validates SOT lengths, tile-part order, and tile-part header
    /// overrides without decoding packet data.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` if the tile progress table exceeds the caller's
    /// working-memory bound.
    pub fn tile_parts(&self) -> Result<TilePartReader<'a>, Jpeg2000Error> {
        TilePartReader::new(self.header().main(), self.options.limits)
    }
}

impl<'a> Decoder<'a, HeaderParsed<'a>> {
    /// Reconstructs every tile without its `levels` highest resolution levels.
    ///
    /// Each discarded level halves the decoded extent along both axes and
    /// skips the code-blocks that would refine it. A component with fewer
    /// decomposition levels is reconstructed from its lowest level alone.
    /// Sinks learn the reduction from [`crate::ComponentPlane::discarded_levels`].
    pub fn discard_levels(mut self, levels: u8) -> Self {
        self.state.progress.discarded_levels = levels;
        self
    }

    /// Returns the fewest resolution levels to discard so that the
    /// coefficient planes of the largest tile fit the working-memory bound.
    ///
    /// The estimate reads the main header's coding parameters and allows
    /// real-valued coefficients throughout, leaving a quarter of
    /// [`crate::DecoderLimits::max_working_bytes`] for the decoder's other
    /// storage. A tile-part header can still override those parameters, so
    /// the decode may yet exceed the bound. It never returns more levels than
    /// every component has.
    pub fn levels_to_fit(&self) -> u8 {
        let main = self.header().main();
        let size = main.size();
        let limit = self.options.limits.max_working_bytes;
        let budget = u64::try_from(limit.saturating_sub(limit / 4)).unwrap_or(u64::MAX);
        let tile = size.tile_size();
        let image = size.image_size();
        let width = u64::from(tile.width.min(image.width));
        let height = u64::from(tile.height.min(image.height));
        let available = (0..size.component_count())
            .map(|component| {
                main.component_parameters(component)
                    .map_or(0, |parameters| parameters.levels)
            })
            .min()
            .unwrap_or(0);
        let plane_bytes = |levels: u8| {
            size.components().fold(0u64, |total, component| {
                let scale = 1u64.checked_shl(u32::from(levels)).unwrap_or(u64::MAX);
                let extent = |length: u64, subsampling: u8| {
                    length
                        .div_ceil(u64::from(subsampling.max(1)))
                        .div_ceil(scale)
                };
                let samples = extent(width, component.x_subsampling)
                    .saturating_mul(extent(height, component.y_subsampling));
                total.saturating_add(samples.saturating_mul(REAL_SAMPLE_BYTES))
            })
        };
        (0..available)
            .find(|levels| plane_bytes(*levels) <= budget)
            .unwrap_or(available)
    }

    /// Reconstructs and emits the first tile to a caller-provided sink.
    ///
    /// The successful transition proves that at least one tile was emitted.
    /// Any sample views passed to `sink` expire on callback return.
    ///
    /// # Errors
    ///
    /// Returns a structural error for malformed packet, code-block, or
    /// coding-parameter data, `LimitExceeded` when the tile's working storage
    /// passes the caller's bound, and whatever the sink returns.
    pub fn decode_next_tile(
        mut self,
        sink: &mut dyn TileSink,
    ) -> Result<Decoder<'a, TileDecoded<'a>>, Jpeg2000Error> {
        self.state.progress.decode_tile(sink)?;
        Ok(Decoder {
            input: self.input,
            options: self.options,
            state: TileDecoded {
                progress: self.state.progress,
            },
        })
    }
}

impl<'a> Decoder<'a, TileDecoded<'a>> {
    /// Reconstructs and emits another tile while retaining the decoded state.
    ///
    /// Tile-part bytes remain borrowed and temporary sample views expire when
    /// the sink returns.
    ///
    /// # Errors
    ///
    /// Returns a structural error for malformed tile data, `LimitExceeded`
    /// when the tile's working storage passes the caller's bound, and
    /// whatever the sink returns.
    pub fn decode_next_tile(mut self, sink: &mut dyn TileSink) -> Result<Self, Jpeg2000Error> {
        self.state.progress.decode_tile(sink)?;
        Ok(self)
    }

    /// Validates that every tile was delivered, then completes the decode.
    ///
    /// Tile-part completeness and the end-of-codestream marker are checked
    /// while the header is parsed, because the tile-parts are framed there.
    ///
    /// # Errors
    ///
    /// Returns `Truncated` if a tile is still outstanding.
    pub fn finish(self) -> Result<Decoder<'a, Complete<'a>>, Jpeg2000Error> {
        if self.state.progress.remaining_tiles > 0 {
            return self
                .header()
                .main()
                .body_site()
                .reject_truncated("tile data");
        }
        Ok(Decoder {
            input: self.input,
            options: self.options,
            state: Complete {
                progress: self.state.progress,
            },
        })
    }
}
