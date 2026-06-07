//! JBIG2 arithmetic text-region decoder.

use crate::{
    arith_decoder::{JBig2ArithDecoder, JBig2ArithIntegerContext},
    compose_op::ComposeOp,
    error::Jbig2Error,
    image::JBig2Image,
    segment_context::SegmentDecodeContext,
    text_region::{
        bitmap::initialized_region,
        flags::TextRegionFlagBits,
        geometry::TextRegionGeometry,
        parser::ParsedTextRegion,
        state::{TextRegionDecodeState, advance_curs_by_delta_s},
    },
    util::{INTEGER_CONVERSION_OVERFLOW, ceil_log2},
};
use pdf_utils::BitReader;

const TEXT_REGION_DELTA_T: &str = "text region delta t";
const TEXT_REGION_FIRST_S_DELTA: &str = "text region first-s delta";
const TEXT_REGION_INSTANCE_T: &str = "text region instance t";
const TEXT_REGION_SYMBOL_ID: &str = "text region symbol id";

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
    parsed: ParsedTextRegion<'a>,
    symbols: Vec<JBig2Image>,
    compose_op: ComposeOp,
    geometry: TextRegionGeometry,
    region: JBig2Image,
    symbol_code_length: u8,
}

impl<'a> ArithmeticTextRegionDecoder<'a> {
    /// Create an arithmetic text-region decoder from parsed segment state.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 permits refinement text
    /// regions, but this decoder currently supports only non-refinement input.
    fn new(
        context: &SegmentDecodeContext<'_, '_, '_, '_, '_>,
        parsed: ParsedTextRegion<'a>,
    ) -> Result<Self, Jbig2Error> {
        if parsed.flags.contains(TextRegionFlagBits::SBREFINE) {
            return Err(Jbig2Error::UnsupportedFeature(
                "arithmetic refinement text regions",
            ));
        }

        let symbols = context.referred_symbol_images()?;
        if symbols.is_empty() {
            return Err(Jbig2Error::MissingSegment);
        }

        let compose_op = ComposeOp::from(parsed.flags.sbcombop_bits());
        let geometry = TextRegionGeometry::from_flags(parsed.flags)?;
        let region = initialized_region(&parsed)?;
        let symbol_code_length = ceil_log2(symbols.len())?;

        Ok(Self {
            parsed,
            compose_op,
            geometry,
            region,
            symbol_code_length,
            symbols,
        })
    }

    /// Decode all arithmetic-coded symbol instances into the region bitmap.
    ///
    /// This implements ITU-T T.88 | ISO/IEC 14492 section 6.4.5 from initial
    /// `STRIPT` setup through the strip and symbol-instance loops.
    fn decode(mut self) -> Result<JBig2Image, Jbig2Error> {
        let mut body_reader = BitReader::new(self.parsed.body);
        let mut decoder = JBig2ArithDecoder::new(&mut body_reader);
        let initial_stript = decoder
            .decode_required_integer(JBig2ArithIntegerContext::TextDeltaT, TEXT_REGION_DELTA_T)?;
        let mut state = TextRegionDecodeState::from_initial_delta(
            initial_stript,
            self.parsed.flags.sbstrips(),
        )?;

        while !state.is_complete(self.parsed.symbol_instances) {
            self.decode_strip(&mut decoder, &mut state)?;
        }

        Ok(self.region)
    }

    /// Decode one arithmetic strip.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(b) decodes `DT`, and
    /// step 3(c) decodes the first and subsequent symbol instances.
    fn decode_strip(
        &mut self,
        decoder: &mut JBig2ArithDecoder<'_, '_>,
        state: &mut TextRegionDecodeState,
    ) -> Result<(), Jbig2Error> {
        let delta_t = decoder
            .decode_required_integer(JBig2ArithIntegerContext::TextDeltaT, TEXT_REGION_DELTA_T)?;
        state.advance_strip(delta_t, self.parsed.flags.sbstrips())?;
        let delta_first_s = decoder.decode_required_integer(
            JBig2ArithIntegerContext::TextFirstS,
            TEXT_REGION_FIRST_S_DELTA,
        )?;
        let mut curs = state.consume_first_s_delta(delta_first_s)?;

        loop {
            curs = self.decode_instance(decoder, state, curs)?;
            if state.is_complete(self.parsed.symbol_instances) {
                break;
            }

            let Some(delta_s) = decoder.decode_integer(JBig2ArithIntegerContext::TextDeltaS)?
            else {
                break;
            };
            curs = advance_curs_by_delta_s(curs, delta_s, self.parsed.flags.sbdsoffset())?;
        }

        Ok(())
    }

    /// Decode and draw one arithmetic symbol instance.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 sections 6.4.9 and 6.4.10 define `CURT`
    /// and IAID symbol-ID decoding before section 6.4.5 placement.
    fn decode_instance(
        &mut self,
        decoder: &mut JBig2ArithDecoder<'_, '_>,
        state: &mut TextRegionDecodeState,
        curs: i64,
    ) -> Result<i64, Jbig2Error> {
        let current_t = if self.parsed.flags.sbstrips() == 1 {
            0
        } else {
            i64::from(decoder.decode_required_integer(
                JBig2ArithIntegerContext::TextInstanceT,
                TEXT_REGION_INSTANCE_T,
            )?)
        };
        let ti = state
            .stript()
            .checked_add(current_t)
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        let symbol_id = usize::try_from(decoder.decode_iaid(self.symbol_code_length)?)
            .map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
        self.draw_instance(symbol_id, curs, ti, state)
    }

    /// Compose one decoded symbol and return the next `CURS` value.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c) adjusts `CURS`,
    /// places the symbol, composes the bitmap, advances `CURS`, and increments
    /// `NINSTANCES`.
    fn draw_instance(
        &mut self,
        symbol_id: usize,
        curs: i64,
        ti: i64,
        state: &mut TextRegionDecodeState,
    ) -> Result<i64, Jbig2Error> {
        let symbol = self
            .symbols
            .get(symbol_id)
            .ok_or(Jbig2Error::InvalidState(TEXT_REGION_SYMBOL_ID))?;
        let placed_curs =
            self.geometry
                .adjust_curs_before_placement(curs, symbol.width(), symbol.height())?;
        let placement =
            self.geometry
                .placement_for(placed_curs, ti, symbol.width(), symbol.height())?;
        symbol.compose_clipped_to(&mut self.region, placement.x, placement.y, self.compose_op);
        let curs = self.geometry.advance_curs_after_placement(
            placed_curs,
            symbol.width(),
            symbol.height(),
        )?;
        state.record_instance()?;
        Ok(curs)
    }
}
