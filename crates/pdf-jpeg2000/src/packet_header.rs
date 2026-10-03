//! Decoding of one packet header into code-block contributions.
//!
//! Annex B.10 gives a packet header a fixed shape: a non-empty bit, then, for
//! every subband of the resolution level and every code-block of that band's
//! precinct, an inclusion decision, the missing bit-plane count on first
//! inclusion, the number of new coding passes, and the byte length of each
//! codeword segment those passes occupy.
//!
//! The per-code-block state and the two tag trees live in [`PrecinctBand`] and
//! persist across the layers of a precinct, because a tag tree is refined by
//! each successive layer rather than restarted.

use pdf_graphics::Size;

use crate::{
    Jpeg2000Error,
    code_block_grid::CodeBlockGrid,
    coding::CodeBlockFlags,
    pass_lengths::{
        SegmentSplit, initial_length_bits, read_length_bits, read_pass_count, read_segment_length,
    },
    stuffed_bits::{HeaderSource, StuffedBitReader},
    tag_tree::TagTree,
    workspace::Workspace,
};

/// Largest number of missing most significant bit-planes a header may signal.
///
/// Part 1 bounds a code-block's magnitude bits by the component precision plus
/// the guard bits, so a longer run is corrupt input rather than a valid image.
const MAX_ZERO_BIT_PLANES: u32 = 74;

/// State of one code-block that persists across the layers of a precinct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CodeBlockProgress {
    /// Whether the block has contributed to any earlier layer.
    pub(crate) included: bool,
    /// `Lblock`, the current width of the segment length prefix.
    pub(crate) length_bits: u8,
    /// Coding passes decoded in earlier layers.
    pub(crate) passes: u32,
    /// Most significant bit-planes known to hold no coefficient.
    pub(crate) zero_bit_planes: u32,
}

impl Default for CodeBlockProgress {
    fn default() -> Self {
        Self {
            included: false,
            length_bits: initial_length_bits(),
            passes: 0,
            zero_bit_planes: 0,
        }
    }
}

/// The code-blocks of one subband inside one precinct.
#[derive(Debug)]
pub(crate) struct PrecinctBand {
    inclusion: TagTree,
    zero_planes: TagTree,
    blocks: Vec<CodeBlockProgress>,
}

impl PrecinctBand {
    /// Creates the per-layer state for one subband precinct.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` when the tag trees and block table pass the
    /// caller's working-memory bound.
    pub(crate) fn new(
        grid: CodeBlockGrid,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        let extent = Size {
            width: grid.columns(),
            height: grid.rows(),
        };
        let count = usize::try_from(grid.count()).map_err(|_| Jpeg2000Error::Overflow {
            context: "code-block count",
        })?;
        Ok(Self {
            inclusion: TagTree::new(extent, workspace)?,
            zero_planes: TagTree::new(extent, workspace)?,
            blocks: workspace.vector(count)?,
        })
    }

    /// Returns whether the precinct holds any code-block at all.
    fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

/// One codeword segment a code-block contributes to the current packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PacketSegment {
    /// Index of the subband within its resolution level.
    pub(crate) band: usize,
    /// Raster index of the code-block within its subband precinct.
    pub(crate) block: u32,
    /// Coding passes carried by this segment.
    pub(crate) passes: u32,
    /// Byte length of the segment in the packet body.
    pub(crate) length: usize,
    /// Most significant bit-planes the block's tag tree resolved as empty.
    pub(crate) zero_bit_planes: u32,
}

/// The codeword segments named by one packet header, in body order.
///
/// The accumulator is reused across packets so reading a tile allocates its
/// segment list once.
#[derive(Debug, Default)]
pub(crate) struct PacketContributions {
    segments: Vec<PacketSegment>,
}

impl PacketContributions {
    /// Reads one packet header and records what each code-block contributes.
    ///
    /// # Errors
    ///
    /// Returns a structural error if the header is truncated, names a
    /// code-block outside its precinct, or signals an impossible length.
    pub(crate) fn read<S: HeaderSource>(
        &mut self,
        reader: &mut StuffedBitReader<S>,
        layer: u16,
        style: CodeBlockFlags,
        bands: &mut [PrecinctBand],
    ) -> Result<(), Jpeg2000Error> {
        self.segments.clear();
        if !reader.read_bit()? {
            reader.align()?;
            return Ok(());
        }
        for (index, band) in bands.iter_mut().enumerate() {
            if band.is_empty() {
                continue;
            }
            self.read_band(reader, layer, style, index, band)?;
        }
        reader.align()
    }

    /// Returns the segments of the packet in the order its body carries them.
    pub(crate) fn segments(&self) -> &[PacketSegment] {
        &self.segments
    }

    /// Reads every code-block of one subband precinct.
    fn read_band<S: HeaderSource>(
        &mut self,
        reader: &mut StuffedBitReader<S>,
        layer: u16,
        style: CodeBlockFlags,
        band: usize,
        state: &mut PrecinctBand,
    ) -> Result<(), Jpeg2000Error> {
        let count = state.blocks.len();
        for index in 0..count {
            let block = u32::try_from(index).map_err(|_| Jpeg2000Error::Overflow {
                context: "code-block index",
            })?;
            let Some(progress) = state.blocks.get(index).copied() else {
                continue;
            };
            if !Self::read_inclusion(
                reader,
                layer,
                block,
                progress.included,
                &mut state.inclusion,
            )? {
                continue;
            }
            let progress = Self::first_inclusion(reader, block, progress, &mut state.zero_planes)?;
            let updated = self.read_contribution(reader, style, band, block, progress)?;
            let slot = state.blocks.get_mut(index).ok_or(Jpeg2000Error::Overflow {
                context: "code-block index",
            })?;
            *slot = updated;
        }
        Ok(())
    }

    /// Reads whether a code-block contributes to the current layer.
    fn read_inclusion<S: HeaderSource>(
        reader: &mut StuffedBitReader<S>,
        layer: u16,
        block: u32,
        included: bool,
        inclusion: &mut TagTree,
    ) -> Result<bool, Jpeg2000Error> {
        if included {
            return reader.read_bit();
        }
        let threshold = u32::from(layer).saturating_add(1);
        inclusion.decode(reader, block, threshold)
    }

    /// Reads the missing bit-plane count when a code-block first contributes.
    fn first_inclusion<S: HeaderSource>(
        reader: &mut StuffedBitReader<S>,
        block: u32,
        progress: CodeBlockProgress,
        zero_planes: &mut TagTree,
    ) -> Result<CodeBlockProgress, Jpeg2000Error> {
        if progress.included {
            return Ok(progress);
        }
        let mut threshold = 1u32;
        while !zero_planes.decode(reader, block, threshold)? {
            threshold = threshold.saturating_add(1);
            if threshold > MAX_ZERO_BIT_PLANES {
                return Err(Jpeg2000Error::Overflow {
                    context: "missing bit-plane count",
                });
            }
        }
        Ok(CodeBlockProgress {
            included: true,
            zero_bit_planes: threshold.saturating_sub(1),
            ..progress
        })
    }

    /// Reads one code-block's pass count and codeword segment lengths.
    fn read_contribution<S: HeaderSource>(
        &mut self,
        reader: &mut StuffedBitReader<S>,
        style: CodeBlockFlags,
        band: usize,
        block: u32,
        progress: CodeBlockProgress,
    ) -> Result<CodeBlockProgress, Jpeg2000Error> {
        let passes = read_pass_count(reader)?;
        let length_bits = read_length_bits(reader, progress.length_bits)?;
        let split = SegmentSplit::new(style);
        let mut start = progress.passes;
        for count in split.segments(progress.passes, passes) {
            let length = read_segment_length(reader, length_bits, count)?;
            self.segments.push(PacketSegment {
                band,
                block,
                passes: count,
                length,
                zero_bit_planes: progress.zero_bit_planes,
            });
            start = start.saturating_add(count);
        }
        Ok(CodeBlockProgress {
            included: true,
            length_bits,
            passes: start,
            zero_bit_planes: progress.zero_bit_planes,
        })
    }
}
