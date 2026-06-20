use crate::{
    arith_decoder::JBig2ArithDecoder,
    compose_op::ComposeOp,
    error::Jbig2Error,
    generic_refinement_region::{
        GenericRefinementRegionDecode, RefinementAdaptiveTemplate, RefinementTemplate,
    },
    image::JBig2Image,
    segment_context::SegmentDecodeContext,
    text_region::{
        bitmap::initialized_region, flags::TextRegionFlagBits, geometry::TextRegionGeometry,
        parser::ParsedTextRegion, state::TextRegionDecodeState,
    },
    util::{INTEGER_CONVERSION_OVERFLOW, refined_dimension},
};

const TEXT_REGION_SYMBOL_ID: &str = "text region symbol id";

/// Decoded symbol-instance coordinates and symbol ID.
///
/// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c) derives `CURS`, `TI`,
/// and the symbol ID before bitmap lookup and composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecodedTextRegionInstance {
    /// Current symbol-instance `S` position for section 6.4.5 composition.
    pub(crate) curs: i64,
    /// Current symbol-instance `T` position for section 6.4.5 composition.
    pub(crate) ti: i64,
    /// Referenced symbol dictionary entry selected for this instance.
    pub(crate) symbol_id: usize,
    /// Whether this instance is refinement-coded before composition.
    pub(crate) refined: bool,
}

/// Shared placement state for text-region decoders.
#[derive(Debug)]
pub(crate) struct TextRegionPlacementContext<'a> {
    parsed: ParsedTextRegion<'a>,
    symbols: Vec<JBig2Image>,
    compose_op: ComposeOp,
    geometry: TextRegionGeometry,
    region: JBig2Image,
}

impl<'a> TextRegionPlacementContext<'a> {
    /// Resolve symbol dictionaries and common placement state.
    pub(crate) fn new(
        context: &SegmentDecodeContext<'_, '_, '_, '_, '_>,
        parsed: ParsedTextRegion<'a>,
    ) -> Result<Self, Jbig2Error> {
        let symbols = context.referred_symbol_images()?;
        if symbols.is_empty() {
            return Err(Jbig2Error::MissingSegment);
        }

        Ok(Self {
            compose_op: ComposeOp::from(parsed.flags.sbcombop_bits()),
            geometry: TextRegionGeometry::from_flags(parsed.flags)?,
            region: initialized_region(&parsed)?,
            parsed,
            symbols,
        })
    }

    /// Return parsed segment state.
    pub(crate) fn parsed(&self) -> ParsedTextRegion<'a> {
        self.parsed
    }

    /// Return the number of referred symbols available for placement.
    pub(crate) fn symbols_len(&self) -> usize {
        self.symbols.len()
    }

    /// Compute the instance `TI` position from `STRIPT` and `CURT`.
    pub(crate) fn instance_ti(&self, stript: i64, current_t: i64) -> Result<i64, Jbig2Error> {
        stript
            .checked_add(current_t)
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
    }

    /// Return the arithmetic-refinement config derived from parsed flags.
    pub(crate) fn refinement_config(&self) -> TextRegionRefinementConfig {
        TextRegionRefinementConfig::from_flags(
            self.parsed.flags.contains(TextRegionFlagBits::SBRTEMPLATE),
            self.parsed.refinement_at,
        )
    }

    /// Compose one referenced symbol instance by symbol ID.
    pub(crate) fn compose_symbol(
        &mut self,
        symbol_id: usize,
        state: &mut TextRegionDecodeState,
        curs: i64,
        ti: i64,
    ) -> Result<i64, Jbig2Error> {
        let symbol = self
            .symbols
            .get(symbol_id)
            .ok_or(Jbig2Error::InvalidState(TEXT_REGION_SYMBOL_ID))?
            .clone();
        self.compose_image(state, curs, ti, &symbol)
    }

    /// Return a referenced symbol image by symbol ID.
    pub(crate) fn symbol(&self, symbol_id: usize) -> Result<&JBig2Image, Jbig2Error> {
        self.symbols
            .get(symbol_id)
            .ok_or(Jbig2Error::InvalidState(TEXT_REGION_SYMBOL_ID))
    }

    /// Compose one decoded bitmap instance.
    pub(crate) fn compose_image(
        &mut self,
        state: &mut TextRegionDecodeState,
        curs: i64,
        ti: i64,
        image: &JBig2Image,
    ) -> Result<i64, Jbig2Error> {
        compose_text_region_instance(
            &mut self.region,
            self.compose_op,
            self.geometry,
            state,
            curs,
            ti,
            image,
        )
    }

    /// Consume the context and return the rendered text-region bitmap.
    pub(crate) fn into_region(self) -> JBig2Image {
        self.region
    }
}

/// Shared decode context for text-region decoders.
#[derive(Debug)]
pub(crate) struct TextRegionDecodeContext<'a> {
    placement: TextRegionPlacementContext<'a>,
}

impl<'a> TextRegionDecodeContext<'a> {
    /// Resolve symbol dictionaries and common placement state.
    pub(crate) fn new(
        context: &SegmentDecodeContext<'_, '_, '_, '_, '_>,
        parsed: ParsedTextRegion<'a>,
    ) -> Result<Self, Jbig2Error> {
        Ok(Self {
            placement: TextRegionPlacementContext::new(context, parsed)?,
        })
    }

    /// Return parsed segment state.
    pub(crate) fn parsed(&self) -> ParsedTextRegion<'a> {
        self.placement.parsed()
    }

    /// Return the number of referred symbols available for placement.
    pub(crate) fn symbols_len(&self) -> usize {
        self.placement.symbols_len()
    }

    /// Compute the instance `TI` position from `STRIPT` and `CURT`.
    pub(crate) fn instance_ti(&self, stript: i64, current_t: i64) -> Result<i64, Jbig2Error> {
        self.placement.instance_ti(stript, current_t)
    }

    /// Compose one referenced symbol instance by symbol ID.
    pub(crate) fn compose_symbol(
        &mut self,
        symbol_id: usize,
        state: &mut TextRegionDecodeState,
        curs: i64,
        ti: i64,
    ) -> Result<i64, Jbig2Error> {
        self.placement.compose_symbol(symbol_id, state, curs, ti)
    }

    /// Compose one decoded bitmap instance.
    pub(crate) fn compose_image(
        &mut self,
        state: &mut TextRegionDecodeState,
        curs: i64,
        ti: i64,
        image: &JBig2Image,
    ) -> Result<i64, Jbig2Error> {
        self.placement.compose_image(state, curs, ti, image)
    }

    /// Compose one previously decoded non-refined instance.
    pub(crate) fn draw_decoded_symbol(
        &mut self,
        state: &mut TextRegionDecodeState,
        instance: DecodedTextRegionInstance,
    ) -> Result<i64, Jbig2Error> {
        self.compose_symbol(instance.symbol_id, state, instance.curs, instance.ti)
    }

    /// Decode one refinement bitmap instance from the referenced symbol.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn decode_refined_image(
        &self,
        symbol_id: usize,
        delta_width: i32,
        delta_height: i32,
        delta_x: i32,
        delta_y: i32,
        width_label: &'static str,
        height_label: &'static str,
        reference_offset: impl Fn(i32, i32) -> Result<i32, Jbig2Error>,
        decoder: &mut JBig2ArithDecoder<'_, '_>,
    ) -> Result<JBig2Image, Jbig2Error> {
        let config = self.placement.refinement_config();
        decode_text_region_refinement_image(
            self.placement.symbol(symbol_id)?,
            delta_width,
            delta_height,
            delta_x,
            delta_y,
            width_label,
            height_label,
            config,
            reference_offset,
            decoder,
        )
    }

    /// Compose one previously decoded refined instance.
    pub(crate) fn draw_refined_image(
        &mut self,
        state: &mut TextRegionDecodeState,
        instance: DecodedTextRegionInstance,
        image: &JBig2Image,
    ) -> Result<i64, Jbig2Error> {
        self.compose_image(state, instance.curs, instance.ti, image)
    }

    /// Consume the context and return the rendered text-region bitmap.
    pub(crate) fn into_region(self) -> JBig2Image {
        self.placement.into_region()
    }
}

/// Template and adaptive-template parameters for text-region refinement.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TextRegionRefinementConfig {
    /// Refinement template selected from text-region flags.
    pub(crate) template: RefinementTemplate,
    /// Adaptive template offsets used by the refinement decoder.
    pub(crate) at: RefinementAdaptiveTemplate,
}

impl TextRegionRefinementConfig {
    /// Build a text-region refinement configuration from parsed flags.
    pub(crate) fn from_flags(
        sbtemplate: bool,
        refinement_at: Option<RefinementAdaptiveTemplate>,
    ) -> Self {
        let template = RefinementTemplate::from_flag(sbtemplate);
        let at = refinement_at.unwrap_or_else(|| RefinementAdaptiveTemplate::default_for(template));
        Self { template, at }
    }
}

/// Compute refined text-region dimensions from a reference bitmap and deltas.
pub(crate) fn refined_text_region_dimensions(
    reference: &JBig2Image,
    delta_width: i32,
    delta_height: i32,
    width_label: &'static str,
    height_label: &'static str,
) -> Result<(u16, u16), Jbig2Error> {
    let width = refined_dimension(reference.width(), delta_width, width_label)?;
    let height = refined_dimension(reference.height(), delta_height, height_label)?;
    Ok((width, height))
}

/// Decode a refinement bitmap for a text-region symbol instance.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_text_region_refinement_image(
    reference: &JBig2Image,
    delta_width: i32,
    delta_height: i32,
    delta_x: i32,
    delta_y: i32,
    width_label: &'static str,
    height_label: &'static str,
    config: TextRegionRefinementConfig,
    reference_offset: impl Fn(i32, i32) -> Result<i32, Jbig2Error>,
    decoder: &mut JBig2ArithDecoder<'_, '_>,
) -> Result<JBig2Image, Jbig2Error> {
    let (width, height) = refined_text_region_dimensions(
        reference,
        delta_width,
        delta_height,
        width_label,
        height_label,
    )?;
    let reference_dx = reference_offset(delta_width, delta_x)?;
    let reference_dy = reference_offset(delta_height, delta_y)?;
    GenericRefinementRegionDecode::new(
        width,
        height,
        config.template,
        false,
        config.at,
        reference_dx,
        reference_dy,
    )
    .decode(reference, decoder)
}

/// Compose one decoded text-region instance and advance `CURS`.
pub(crate) fn compose_text_region_instance(
    region: &mut JBig2Image,
    compose_op: ComposeOp,
    geometry: TextRegionGeometry,
    state: &mut TextRegionDecodeState,
    curs: i64,
    ti: i64,
    image: &JBig2Image,
) -> Result<i64, Jbig2Error> {
    let placed_curs = geometry.adjust_curs_before_placement(curs, image.width(), image.height())?;
    let placement = geometry.placement_for(placed_curs, ti, image.width(), image.height())?;
    image.compose_clipped_to(region, placement.x, placement.y, compose_op);
    let curs = geometry.advance_curs_after_placement(placed_curs, image.width(), image.height())?;
    state.record_instance()?;
    Ok(curs)
}

#[cfg(test)]
mod tests {
    use super::{
        DecodedTextRegionInstance, TextRegionDecodeContext, TextRegionPlacementContext,
        TextRegionRefinementConfig, compose_text_region_instance, refined_text_region_dimensions,
    };
    use crate::{
        compose_op::ComposeOp,
        error::Jbig2Error,
        generic_refinement_region::{RefinementAdaptiveTemplate, RefinementTemplate},
        image::JBig2Image,
        region_info::RegionInfo,
        segment::{JBig2SegmentResult, ParsedSegment},
        segment_context::SegmentDecodeContext,
        symbol_dictionary::SymbolDictionary,
        text_region::{
            geometry::{TextRegionGeometry, TextRegionRefCorner},
            parser::ParsedTextRegion,
            state::TextRegionDecodeState,
            strip_decode_driver::{
                TextRegionRefinedInstanceDecodeDriver, TextRegionStripDecodeDriver,
                decode_text_region,
            },
        },
    };

    struct ScriptedStripDriver {
        context: TextRegionDecodeContext<'static>,
        state: TextRegionDecodeState,
        strip_deltas: Vec<i32>,
        first_symbol_deltas: Vec<i32>,
        delta_s_values: Vec<Option<i32>>,
        current_t_values: Vec<i64>,
        symbol_ids: Vec<usize>,
        refinement_flags: Vec<bool>,
        refined_images: Vec<JBig2Image>,
        non_refined: Vec<DecodedTextRegionInstance>,
    }

    impl ScriptedStripDriver {
        fn new(parsed: ParsedTextRegion<'static>) -> Self {
            let symbol = JBig2Image::new(1, 1);
            let referred = ParsedSegment {
                number: 1,
                flags: 0,
                referred_to_segment_numbers: vec![],
                page_association: 0,
                data_length: None,
                result: JBig2SegmentResult::SymbolDictionary(SymbolDictionary {
                    images: vec![symbol],
                }),
            };
            let segment = ParsedSegment {
                number: 2,
                flags: 0,
                referred_to_segment_numbers: vec![1],
                page_association: 0,
                data_length: None,
                result: JBig2SegmentResult::None,
            };
            let mut stream = pdf_utils::BitReader::new(&[]);
            let prior_segments = [referred];
            let context = SegmentDecodeContext::new(&segment, &mut stream, 0, &[], &prior_segments);

            Self {
                context: TextRegionDecodeContext::new(&context, parsed).expect("context"),
                state: TextRegionDecodeState::from_initial_delta(0, 1).expect("state"),
                strip_deltas: Vec::new(),
                first_symbol_deltas: Vec::new(),
                delta_s_values: Vec::new(),
                current_t_values: Vec::new(),
                symbol_ids: Vec::new(),
                refinement_flags: Vec::new(),
                refined_images: Vec::new(),
                non_refined: Vec::new(),
            }
        }
    }

    impl TextRegionRefinedInstanceDecodeDriver for ScriptedStripDriver {
        fn with_context_and_state<T>(
            &mut self,
            f: impl FnOnce(&mut TextRegionDecodeContext<'_>, &mut TextRegionDecodeState) -> T,
        ) -> T {
            f(&mut self.context, &mut self.state)
        }

        fn decode_refined_instance_image(
            &mut self,
            _instance: DecodedTextRegionInstance,
        ) -> Result<JBig2Image, Jbig2Error> {
            Ok(self.refined_images.remove(0))
        }
    }

    impl TextRegionStripDecodeDriver for ScriptedStripDriver {
        fn context(&self) -> &TextRegionDecodeContext<'_> {
            &self.context
        }

        fn state(&self) -> &TextRegionDecodeState {
            &self.state
        }

        fn state_mut(&mut self) -> &mut TextRegionDecodeState {
            &mut self.state
        }

        fn decode_next_strip_header_delta(&mut self) -> Result<i32, Jbig2Error> {
            Ok(self.strip_deltas.remove(0))
        }

        fn decode_first_symbol_delta(&mut self) -> Result<i32, Jbig2Error> {
            Ok(self.first_symbol_deltas.remove(0))
        }

        fn decode_delta_s_or_end(&mut self) -> Result<Option<i32>, Jbig2Error> {
            Ok(self.delta_s_values.remove(0))
        }

        fn decode_current_t(&mut self) -> Result<i64, Jbig2Error> {
            Ok(self.current_t_values.remove(0))
        }

        fn decode_symbol_id(&mut self) -> Result<usize, Jbig2Error> {
            Ok(self.symbol_ids.remove(0))
        }

        fn decode_refinement_flag(&mut self) -> Result<bool, Jbig2Error> {
            Ok(self.refinement_flags.remove(0))
        }

        fn draw_non_refined_instance(
            &mut self,
            instance: DecodedTextRegionInstance,
        ) -> Result<(), Jbig2Error> {
            self.non_refined.push(instance);
            self.state.record_instance()
        }
    }

    #[test]
    fn computes_refined_dimensions() {
        let reference = JBig2Image::new(3, 4);
        let dims = refined_text_region_dimensions(&reference, 1, -1, "width", "height")
            .expect("dimensions");

        assert_eq!(dims, (4, 3));
    }

    #[test]
    fn composes_text_region_instance_and_advances_curs() {
        let mut region = JBig2Image::new(2, 2);
        let image = JBig2Image::new(1, 1);
        let geometry = TextRegionGeometry::new(false, TextRegionRefCorner::TopLeft);
        let mut state = TextRegionDecodeState::from_initial_delta(0, 1).expect("state");

        let curs = compose_text_region_instance(
            &mut region,
            ComposeOp::Or,
            geometry,
            &mut state,
            0,
            0,
            &image,
        )
        .expect("compose");

        assert_eq!(curs, 0);
        assert!(state.is_complete(1));
    }

    #[test]
    fn resolves_refinement_config_from_flags() {
        let config = TextRegionRefinementConfig::from_flags(
            true,
            Some(RefinementAdaptiveTemplate::default_for(
                RefinementTemplate::Template1,
            )),
        );

        assert_eq!(config.template, RefinementTemplate::Template1);
    }

    #[test]
    fn placement_context_rejects_missing_symbols() {
        let parsed = ParsedTextRegion {
            region: RegionInfo {
                width: 1,
                height: 1,
                x: 0,
                y: 0,
                flags: 0,
            },
            flags: crate::text_region::flags::TextRegionFlagBits::from_bits_retain(0),
            huffman_flags: None,
            symbol_instances: 1,
            refinement_at: None,
            body: &[],
        };
        let segment = ParsedSegment {
            number: 1,
            flags: 0,
            referred_to_segment_numbers: vec![],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::None,
        };
        let mut stream = pdf_utils::BitReader::new(&[]);
        let context = SegmentDecodeContext::new(&segment, &mut stream, 0, &[], &[]);

        let err = TextRegionPlacementContext::new(&context, parsed).expect_err("missing symbols");

        assert!(matches!(err, crate::error::Jbig2Error::MissingSegment));
    }

    #[test]
    fn placement_context_errors_for_invalid_symbol_id() {
        let parsed = ParsedTextRegion {
            region: RegionInfo {
                width: 2,
                height: 2,
                x: 0,
                y: 0,
                flags: 0,
            },
            flags: crate::text_region::flags::TextRegionFlagBits::from_bits_retain(0),
            huffman_flags: None,
            symbol_instances: 1,
            refinement_at: None,
            body: &[],
        };
        let symbol = JBig2Image::new(1, 1);
        let referred = ParsedSegment {
            number: 1,
            flags: 0,
            referred_to_segment_numbers: vec![],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::SymbolDictionary(SymbolDictionary {
                images: vec![symbol],
            }),
        };
        let segment = ParsedSegment {
            number: 2,
            flags: 0,
            referred_to_segment_numbers: vec![1],
            page_association: 0,
            data_length: None,
            result: JBig2SegmentResult::None,
        };
        let mut stream = pdf_utils::BitReader::new(&[]);
        let prior_segments = [referred];
        let context = SegmentDecodeContext::new(&segment, &mut stream, 0, &[], &prior_segments);
        let mut placement = TextRegionPlacementContext::new(&context, parsed).expect("context");
        let mut state = TextRegionDecodeState::from_initial_delta(0, 1).expect("state");

        let err = placement
            .compose_symbol(3, &mut state, 0, 0)
            .expect_err("invalid symbol id");

        assert!(matches!(
            err,
            crate::error::Jbig2Error::InvalidState("text region symbol id")
        ));
    }

    #[test]
    fn default_driver_flow_decodes_first_and_subsequent_symbols() {
        let parsed = ParsedTextRegion {
            region: RegionInfo {
                width: 2,
                height: 2,
                x: 0,
                y: 0,
                flags: 0,
            },
            flags: crate::text_region::flags::TextRegionFlagBits::from_bits_retain(0),
            huffman_flags: None,
            symbol_instances: 2,
            refinement_at: None,
            body: &[],
        };
        let mut driver = ScriptedStripDriver::new(parsed);
        driver.strip_deltas = vec![0];
        driver.first_symbol_deltas = vec![5];
        driver.delta_s_values = vec![Some(3)];
        driver.current_t_values = vec![1, 2];
        driver.symbol_ids = vec![0, 0];
        driver.refinement_flags = vec![false, false];

        decode_text_region(&mut driver).expect("decode text region");

        assert_eq!(driver.non_refined.len(), 2);
        assert_eq!(driver.non_refined[0].curs, 5);
        assert_eq!(driver.non_refined[0].ti, 1);
        assert_eq!(driver.non_refined[1].curs, 8);
        assert_eq!(driver.non_refined[1].ti, 2);
        assert_eq!(driver.refined_images.len(), 0);
        assert!(driver.is_complete());
    }

    #[test]
    fn default_driver_flow_stops_on_out_of_band_delta_s() {
        let parsed = ParsedTextRegion {
            region: RegionInfo {
                width: 2,
                height: 2,
                x: 0,
                y: 0,
                flags: 0,
            },
            flags: crate::text_region::flags::TextRegionFlagBits::from_bits_retain(0),
            huffman_flags: None,
            symbol_instances: 2,
            refinement_at: None,
            body: &[],
        };
        let mut driver = ScriptedStripDriver::new(parsed);
        driver.strip_deltas = vec![0, 0];
        driver.first_symbol_deltas = vec![2, 4];
        driver.delta_s_values = vec![None];
        driver.current_t_values = vec![0, 0];
        driver.symbol_ids = vec![0, 0];
        driver.refinement_flags = vec![false, false];

        decode_text_region(&mut driver).expect("decode text region");

        assert_eq!(driver.non_refined.len(), 2);
        assert_eq!(driver.non_refined[0].curs, 2);
        assert_eq!(driver.non_refined[1].curs, 6);
        assert_eq!(driver.first_symbol_deltas.len(), 0);
        assert_eq!(driver.delta_s_values.len(), 0);
        assert!(driver.is_complete());
    }

    #[test]
    fn default_driver_dispatches_refined_instances() {
        let parsed = ParsedTextRegion {
            region: RegionInfo {
                width: 2,
                height: 2,
                x: 0,
                y: 0,
                flags: 0,
            },
            flags: crate::text_region::flags::TextRegionFlagBits::SBREFINE,
            huffman_flags: None,
            symbol_instances: 1,
            refinement_at: None,
            body: &[],
        };
        let mut driver = ScriptedStripDriver::new(parsed);
        driver.strip_deltas = vec![0];
        driver.first_symbol_deltas = vec![1];
        driver.current_t_values = vec![0];
        driver.symbol_ids = vec![0];
        driver.refinement_flags = vec![true];
        let mut refined = JBig2Image::new(1, 1);
        refined.set_pixel(0, 0, 1);
        driver.refined_images = vec![refined];

        decode_text_region(&mut driver).expect("decode text region");

        assert!(driver.non_refined.is_empty());
        assert_eq!(driver.refined_images.len(), 0);
        assert_eq!(driver.context.into_region().get_pixel(1, 0), 1);
    }
}
