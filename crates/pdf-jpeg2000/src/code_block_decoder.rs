//! Driving the coding passes of one code-block.
//!
//! Annex D decodes a code-block as a sequence of coding passes: one cleanup
//! pass for the most significant bit-plane that carries anything, then a
//! significance, refinement, and cleanup pass for each plane below it. Tier-2
//! says how many passes arrived and in which codeword segments, and the
//! code-block style says where those segments terminate and which of them are
//! raw rather than arithmetic.
//!
//! A codeword segment reaches the decoder in one piece per quality layer that
//! contributed to it, so the pieces are gathered before the segment is read
//! and the entropy coder runs across them as one stream.

use pdf_graphics::Size;

use crate::{
    Jpeg2000Error,
    code_block_state::CodeBlockState,
    coding::CodeBlockFlags,
    mq_contexts::ContextSet,
    pass_cleanup,
    pass_coder::{PassScope, SegmentCoder},
    pass_lengths::SegmentSplit,
    pass_refinement, pass_significance,
    resolution::BandKind,
    tile_packets::CodewordSegment,
    workspace::Workspace,
};

/// Coding passes in one bit-plane below the first cleanup pass.
const PASSES_PER_PLANE: u32 = 3;
/// First coding pass that the selective bypass may read as raw bits.
const BYPASS_FIRST_RAW_PASS: u32 = 10;

/// Which of the three Annex D passes a pass index names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PassKind {
    /// Significance propagation, the first pass of a bit-plane.
    Significance,
    /// Magnitude refinement, the second pass of a bit-plane.
    Refinement,
    /// Cleanup, which ends a bit-plane.
    Cleanup,
}

impl PassKind {
    /// Returns the pass at an index in a code-block's pass sequence.
    ///
    /// Index zero is the cleanup pass of the first coded bit-plane; every
    /// plane below it contributes the full triple.
    fn of(index: u32) -> Self {
        match index.checked_sub(1).map(|offset| offset % PASSES_PER_PLANE) {
            None | Some(2) => Self::Cleanup,
            Some(0) => Self::Significance,
            Some(_) => Self::Refinement,
        }
    }

    /// Returns whether the selective bypass reads this pass as raw bits.
    fn is_bypassed(self, index: u32, style: CodeBlockFlags) -> bool {
        style.contains(CodeBlockFlags::BYPASS)
            && index >= BYPASS_FIRST_RAW_PASS
            && self != Self::Cleanup
    }
}

/// What Tier-2 learned about one code-block, as Tier-1 needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BlockRequest {
    /// Coefficient extent of the code-block.
    pub(crate) size: Size<u32>,
    /// Orientation of the subband the code-block belongs to.
    pub(crate) band: BandKind,
    /// Code-block style flags from COD or COC.
    pub(crate) style: CodeBlockFlags,
    /// Magnitude bit-planes the subband's quantization allows.
    pub(crate) magnitude_bits: u32,
    /// Most significant bit-planes the packet headers reported as empty.
    pub(crate) zero_bit_planes: u32,
    /// Coding passes the code-block received across every layer.
    pub(crate) passes: u32,
}

impl BlockRequest {
    /// Returns the most significant bit-plane the code-block codes.
    fn first_plane(self) -> Option<u32> {
        self.magnitude_bits
            .checked_sub(self.zero_bit_planes)?
            .checked_sub(1)
    }

    /// Returns the passes to run, capped by the planes actually available.
    fn effective_passes(self, first_plane: u32) -> u32 {
        let available = first_plane
            .saturating_mul(PASSES_PER_PLANE)
            .saturating_add(1);
        self.passes.min(available)
    }
}

/// Decodes code-blocks one at a time, reusing one set of buffers.
#[derive(Debug)]
pub(crate) struct CodeBlockDecoder<'a> {
    state: CodeBlockState,
    contexts: ContextSet,
    chunks: Vec<&'a [u8]>,
}

impl<'a> CodeBlockDecoder<'a> {
    /// Allocates buffers for code-blocks up to `capacity` coefficients across.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` when the buffers pass the caller's
    /// working-memory bound.
    pub(crate) fn new(
        capacity: Size<u32>,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        Ok(Self {
            state: CodeBlockState::new(capacity, workspace)?,
            contexts: ContextSet::default(),
            chunks: Vec::new(),
        })
    }

    /// Returns the coefficients of the code-block decoded last.
    pub(crate) fn state(&self) -> &CodeBlockState {
        &self.state
    }

    /// Decodes one code-block and returns the lowest bit-plane it reached.
    ///
    /// A code-block with no coding passes leaves every coefficient zero and
    /// reports the plane below its first, because nothing was decoded.
    ///
    /// # Errors
    ///
    /// Returns a structural error when a codeword segment runs out of bits,
    /// or when a segmentation symbol shows the segment is corrupt.
    pub(crate) fn decode(
        &mut self,
        request: BlockRequest,
        segments: impl Iterator<Item = CodewordSegment<'a>>,
        workspace: &mut Workspace,
    ) -> Result<u32, Jpeg2000Error> {
        self.state.begin(request.size)?;
        self.contexts.reset();
        let Some(first_plane) = request.first_plane() else {
            return Ok(0);
        };
        let total = request.effective_passes(first_plane);
        let split = SegmentSplit::new(request.style);
        let mut segments = segments;
        let mut pending: Option<CodewordSegment<'a>> = None;
        let mut pass = 0u32;
        while pass < total {
            self.chunks.clear();
            let mut gathered = 0u32;
            let natural = split.segment_passes(pass, u32::MAX);
            while gathered < natural {
                let Some(segment) = pending.take().or_else(|| segments.next()) else {
                    break;
                };
                workspace.push(&mut self.chunks, segment.data)?;
                gathered = gathered.saturating_add(segment.passes);
            }
            if self.chunks.is_empty() {
                break;
            }
            let passes = gathered.min(total.saturating_sub(pass));
            self.run_segment(&request, first_plane, pass, passes)?;
            pass = pass.saturating_add(passes);
        }
        // The pass at index `pass - 1` was the last one decoded, and index
        // `i` belongs to bit-plane `first_plane - (i + 2) / 3`.
        Ok(first_plane.saturating_sub(pass.saturating_add(1) / PASSES_PER_PLANE))
    }

    /// Runs the coding passes carried by one codeword segment.
    fn run_segment(
        &mut self,
        request: &BlockRequest,
        first_plane: u32,
        start: u32,
        passes: u32,
    ) -> Result<(), Jpeg2000Error> {
        let Self {
            state,
            contexts,
            chunks,
        } = self;
        let raw = PassKind::of(start).is_bypassed(start, request.style);
        let mut coder = if raw {
            SegmentCoder::raw(chunks, 0)
        } else {
            SegmentCoder::arithmetic(chunks)
        };
        for offset in 0..passes {
            let index = start.saturating_add(offset);
            let plane = first_plane.saturating_sub(
                index.saturating_add(PASSES_PER_PLANE.saturating_sub(1)) / PASSES_PER_PLANE,
            );
            let scope = PassScope {
                band: request.band,
                plane,
                causal: request.style.contains(CodeBlockFlags::VERTICAL_CAUSAL),
            };
            if request.style.contains(CodeBlockFlags::RESET) {
                contexts.reset();
            }
            match PassKind::of(index) {
                PassKind::Significance => {
                    pass_significance::run(&mut coder, contexts, state, scope)?;
                }
                PassKind::Refinement => {
                    pass_refinement::run(&mut coder, contexts, state, scope)?;
                }
                PassKind::Cleanup => {
                    pass_cleanup::run(
                        &mut coder,
                        contexts,
                        state,
                        scope,
                        request.style.contains(CodeBlockFlags::SEGMENTATION),
                    )?;
                }
            }
        }
        Ok(())
    }
}
