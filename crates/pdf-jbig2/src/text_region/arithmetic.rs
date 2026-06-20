//! JBIG2 arithmetic text-region decoder.

use crate::{
    arith_decoder::{JBig2ArithDecoder, JBig2ArithIntegerContext},
    error::Jbig2Error,
    image::JBig2Image,
    segment_context::SegmentDecodeContext,
    text_region::{
        flags::TextRegionFlagBits,
        parser::ParsedTextRegion,
        refinement::{DecodedTextRegionInstance, TextRegionDecodeContext},
        state::TextRegionDecodeState,
        strip_decode_driver::{
            TextRegionRefinedInstanceDecodeDriver, TextRegionStripDecodeDriver, decode_text_region,
        },
    },
    util::{INTEGER_CONVERSION_OVERFLOW, ceil_log2, refinement_reference_offset},
};
use pdf_utils::BitReader;

const TEXT_REGION_DELTA_T: &str = "text region delta t";
const TEXT_REGION_FIRST_S_DELTA: &str = "text region first-s delta";
const TEXT_REGION_INSTANCE_T: &str = "text region instance t";
const TEXT_REGION_REFINEMENT_FLAG: &str = "text region refinement flag";
const TEXT_REGION_REFINEMENT_WIDTH: &str = "text region refinement width";
const TEXT_REGION_REFINEMENT_HEIGHT: &str = "text region refinement height";
const TEXT_REGION_REFINEMENT_X: &str = "text region refinement x";
const TEXT_REGION_REFINEMENT_Y: &str = "text region refinement y";

/// Decode an arithmetic-coded JBIG2 text-region body.
///
/// ITU-T T.88 | ISO/IEC 14492 section 7.4.3 selects this path when
/// `SBHUFF = 0`; section 6.4.5 defines the shared symbol placement loop.
pub(crate) fn decode_arithmetic_text_region_segment(
    context: &SegmentDecodeContext<'_, '_, '_, '_, '_>,
    parsed: ParsedTextRegion<'_>,
) -> Result<JBig2Image, Jbig2Error> {
    ArithmeticTextRegionDecoder::new(context, parsed)?.decode()
}

/// Decoder for the arithmetic text-region procedure.
///
/// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 defines the symbol-instance
/// placement loop; arithmetic integer contexts supply `DT`, `DFS`, `IDS`,
/// `CURT`, and IAID symbol IDs.
struct ArithmeticTextRegionDecoder<'a> {
    context: TextRegionDecodeContext<'a>,
    symbol_code_length: u8,
}

struct ArithmeticTextRegionStripDecoder<'decoder, 'stream, 'context, 'data> {
    context: &'decoder mut TextRegionDecodeContext<'data>,
    decoder: &'decoder mut JBig2ArithDecoder<'stream, 'context>,
    state: &'decoder mut TextRegionDecodeState,
    symbol_code_length: u8,
}

impl<'a> ArithmeticTextRegionDecoder<'a> {
    /// Create an arithmetic text-region decoder from parsed segment state.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 permits refinement text
    /// regions. When `SBREFINE` is set, individual instances carry a
    /// refinement flag and may decode a temporary refinement bitmap before
    /// composition.
    fn new(
        context: &SegmentDecodeContext<'_, '_, '_, '_, '_>,
        parsed: ParsedTextRegion<'a>,
    ) -> Result<Self, Jbig2Error> {
        let context = TextRegionDecodeContext::new(context, parsed)?;
        let symbol_code_length = ceil_log2(context.symbols_len())?;

        Ok(Self {
            context,
            symbol_code_length,
        })
    }

    /// Decode all arithmetic-coded symbol instances into the region bitmap.
    ///
    /// This implements ITU-T T.88 | ISO/IEC 14492 section 6.4.5 from initial
    /// `STRIPT` setup through the strip and symbol-instance loops.
    fn decode(mut self) -> Result<JBig2Image, Jbig2Error> {
        let mut body_reader = BitReader::new(self.context.parsed().body);
        let mut decoder = JBig2ArithDecoder::new(&mut body_reader);
        let initial_stript = decoder
            .decode_required_integer(JBig2ArithIntegerContext::TextDeltaT, TEXT_REGION_DELTA_T)?;
        let mut state = TextRegionDecodeState::from_initial_delta(
            initial_stript,
            self.context.parsed().flags.sbstrips(),
        )?;
        {
            let mut strip_decoder = ArithmeticTextRegionStripDecoder {
                context: &mut self.context,
                decoder: &mut decoder,
                state: &mut state,
                symbol_code_length: self.symbol_code_length,
            };
            decode_text_region(&mut strip_decoder)?;
        }

        Ok(self.context.into_region())
    }
}

impl TextRegionRefinedInstanceDecodeDriver for ArithmeticTextRegionStripDecoder<'_, '_, '_, '_> {
    fn with_context_and_state<T>(
        &mut self,
        f: impl FnOnce(&mut TextRegionDecodeContext<'_>, &mut TextRegionDecodeState) -> T,
    ) -> T {
        f(self.context, self.state)
    }

    fn decode_refined_instance_image(
        &mut self,
        instance: DecodedTextRegionInstance,
    ) -> Result<JBig2Image, Jbig2Error> {
        let delta_width = self.decoder.decode_required_integer(
            JBig2ArithIntegerContext::RefinementDeltaWidth,
            TEXT_REGION_REFINEMENT_WIDTH,
        )?;
        let delta_height = self.decoder.decode_required_integer(
            JBig2ArithIntegerContext::RefinementDeltaHeight,
            TEXT_REGION_REFINEMENT_HEIGHT,
        )?;
        let delta_x = self.decoder.decode_required_integer(
            JBig2ArithIntegerContext::RefinementDeltaX,
            TEXT_REGION_REFINEMENT_X,
        )?;
        let delta_y = self.decoder.decode_required_integer(
            JBig2ArithIntegerContext::RefinementDeltaY,
            TEXT_REGION_REFINEMENT_Y,
        )?;
        self.context.decode_refined_image(
            instance.symbol_id,
            delta_width,
            delta_height,
            delta_x,
            delta_y,
            TEXT_REGION_REFINEMENT_WIDTH,
            TEXT_REGION_REFINEMENT_HEIGHT,
            refinement_reference_offset,
            self.decoder,
        )
    }
}

impl TextRegionStripDecodeDriver for ArithmeticTextRegionStripDecoder<'_, '_, '_, '_> {
    fn context(&self) -> &TextRegionDecodeContext<'_> {
        self.context
    }

    fn state(&self) -> &TextRegionDecodeState {
        self.state
    }

    fn state_mut(&mut self) -> &mut TextRegionDecodeState {
        self.state
    }

    /// Decode the next strip `DT` header.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 sections 6.4.5 step 3(b) and 6.4.6 define
    /// strip delta decoding and `STRIPT` advancement.
    fn decode_next_strip_header_delta(&mut self) -> Result<i32, Jbig2Error> {
        self.decoder
            .decode_required_integer(JBig2ArithIntegerContext::TextDeltaT, TEXT_REGION_DELTA_T)
    }

    fn decode_first_symbol_delta(&mut self) -> Result<i32, Jbig2Error> {
        self.decoder.decode_required_integer(
            JBig2ArithIntegerContext::TextFirstS,
            TEXT_REGION_FIRST_S_DELTA,
        )
    }

    fn decode_delta_s_or_end(&mut self) -> Result<Option<i32>, Jbig2Error> {
        self.decoder
            .decode_integer(JBig2ArithIntegerContext::TextDeltaS)
    }

    fn decode_current_t(&mut self) -> Result<i64, Jbig2Error> {
        let current_t = if self.context.parsed().flags.sbstrips() == 1 {
            0
        } else {
            i64::from(self.decoder.decode_required_integer(
                JBig2ArithIntegerContext::TextInstanceT,
                TEXT_REGION_INSTANCE_T,
            )?)
        };
        Ok(current_t)
    }

    fn decode_symbol_id(&mut self) -> Result<usize, Jbig2Error> {
        usize::try_from(self.decoder.decode_iaid(self.symbol_code_length)?)
            .map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
    }

    fn decode_refinement_flag(&mut self) -> Result<bool, Jbig2Error> {
        if !self
            .context
            .parsed()
            .flags
            .contains(TextRegionFlagBits::SBREFINE)
        {
            return Ok(false);
        }
        let value = self.decoder.decode_required_integer(
            JBig2ArithIntegerContext::RefinementInstance,
            TEXT_REGION_REFINEMENT_FLAG,
        )?;
        Ok(value != 0)
    }
}
