use crate::{
    error::Jbig2Error,
    generic_refinement_region::{
        GenericRefinementRegionDecode, RefinementAdaptiveTemplate, RefinementTemplate,
    },
    image::JBig2Image,
    symbol_dictionary::{
        current_symbol_set::CurrentSymbolSet, flags::SymbolDictionaryFlagBits,
        header::ParsedSymbolDictionaryHeader,
    },
    text_region::geometry::{TextRegionGeometry, TextRegionRefCorner},
    util::refined_dimension,
};

/// Template configuration shared by symbol-dictionary refinement decoders.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SymbolDictionaryRefinementConfig {
    pub(crate) template: RefinementTemplate,
    pub(crate) at: RefinementAdaptiveTemplate,
}

/// Aggregate symbol-refinement lookup and template parameters.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AggregateRefinementParams<'a> {
    pub(crate) symbols: CurrentSymbolSet<'a>,
    pub(crate) symbol_code_length: u8,
    pub(crate) refinement: SymbolDictionaryRefinementConfig,
}

impl<'a> AggregateRefinementParams<'a> {
    /// Build the aggregate refinement parameters used by both symbol-dictionary decoders.
    pub(crate) fn new(
        symbols: CurrentSymbolSet<'a>,
        symbol_code_length: u8,
        refinement: SymbolDictionaryRefinementConfig,
    ) -> Self {
        Self {
            symbols,
            symbol_code_length,
            refinement,
        }
    }
}

/// Build aggregate refinement lookup and template parameters.
pub(crate) fn aggregate_refinement_params<'a>(
    input_symbols: &'a [JBig2Image],
    new_symbols: &'a [JBig2Image],
    invalid_symbol_label: &'static str,
    symbol_code_length: u8,
    header: &ParsedSymbolDictionaryHeader,
) -> AggregateRefinementParams<'a> {
    AggregateRefinementParams::new(
        CurrentSymbolSet::new(input_symbols, new_symbols, invalid_symbol_label),
        symbol_code_length,
        symbol_dictionary_refinement_config(header),
    )
}

/// Return the aggregate-refinement placement geometry used by symbol dictionaries.
pub(crate) fn aggregate_refinement_geometry() -> TextRegionGeometry {
    TextRegionGeometry::new(false, TextRegionRefCorner::TopLeft)
}

/// Resolve the refinement template and adaptive template from the parsed header.
pub(crate) fn symbol_dictionary_refinement_config(
    header: &ParsedSymbolDictionaryHeader,
) -> SymbolDictionaryRefinementConfig {
    let template = RefinementTemplate::from_flag(
        header
            .flags
            .contains(SymbolDictionaryFlagBits::SDR_TEMPLATE),
    );
    let at = header
        .refinement_at
        .unwrap_or_else(|| RefinementAdaptiveTemplate::default_for(template));
    SymbolDictionaryRefinementConfig { template, at }
}

/// Compute refined symbol dimensions from a reference symbol and width/height deltas.
pub(crate) fn refined_symbol_dimensions(
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

/// Decode a refinement symbol whose size is derived from the reference symbol and deltas.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_refinement_symbol_from_deltas(
    reference: &JBig2Image,
    delta_width: i32,
    delta_height: i32,
    delta_x: i32,
    delta_y: i32,
    width_label: &'static str,
    height_label: &'static str,
    config: SymbolDictionaryRefinementConfig,
    reference_offset: impl Fn(i32, i32) -> Result<i32, Jbig2Error>,
    decoder: &mut crate::arith_decoder::JBig2ArithDecoder<'_, '_>,
) -> Result<JBig2Image, Jbig2Error> {
    let (width, height) = refined_symbol_dimensions(
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

#[cfg(test)]
mod tests {
    use super::{
        SymbolDictionaryRefinementConfig, aggregate_refinement_geometry,
        aggregate_refinement_params, decode_refinement_symbol_from_deltas,
        refined_symbol_dimensions, symbol_dictionary_refinement_config,
    };
    use crate::{
        arith_decoder::JBig2ArithDecoder,
        generic_refinement_region::{
            GenericRefinementRegionDecode, RefinementAdaptiveTemplate, RefinementTemplate,
        },
        image::JBig2Image,
        symbol_dictionary::{
            flags::SymbolDictionaryFlagBits, header::ParsedSymbolDictionaryHeader,
        },
        text_region::geometry::{TextRegionGeometry, TextRegionRefCorner},
    };

    #[test]
    fn resolves_refinement_config_from_header() {
        let header = ParsedSymbolDictionaryHeader {
            flags: SymbolDictionaryFlagBits::SDR_TEMPLATE,
            generic_at: None,
            refinement_at: Some(RefinementAdaptiveTemplate::default_for(
                RefinementTemplate::Template1,
            )),
            num_exported: 0,
            num_new_symbols: 0,
        };

        let config = symbol_dictionary_refinement_config(&header);
        assert_eq!(config.template, RefinementTemplate::Template1);
    }

    #[test]
    fn computes_refined_symbol_dimensions() {
        let reference = JBig2Image::new(3, 4);
        let dims =
            refined_symbol_dimensions(&reference, 1, -1, "width", "height").expect("dimensions");

        assert_eq!(dims, (4, 3));
    }

    #[test]
    fn builds_aggregate_refinement_params() {
        let header = ParsedSymbolDictionaryHeader {
            flags: SymbolDictionaryFlagBits::empty(),
            generic_at: None,
            refinement_at: None,
            num_exported: 0,
            num_new_symbols: 0,
        };
        let params = aggregate_refinement_params(&[], &[], "invalid", 3, &header);

        assert_eq!(params.symbol_code_length, 3);
    }

    #[test]
    fn aggregate_geometry_is_top_left_non_transposed() {
        let geometry = aggregate_refinement_geometry();

        assert_eq!(
            geometry,
            TextRegionGeometry::new(false, TextRegionRefCorner::TopLeft)
        );
    }

    #[test]
    fn decodes_refinement_symbol_with_explicit_size() {
        let reference = JBig2Image::new(1, 1);
        let config = SymbolDictionaryRefinementConfig {
            template: RefinementTemplate::Template0,
            at: RefinementAdaptiveTemplate::default_for(RefinementTemplate::Template0),
        };
        let mut reader = pdf_utils::BitReader::new(&[0x00]);
        let mut decoder = JBig2ArithDecoder::new(&mut reader);

        let image =
            GenericRefinementRegionDecode::new(1, 1, config.template, false, config.at, 0, 0)
                .decode(&reference, &mut decoder)
                .expect("decode");
        assert_eq!((image.width(), image.height()), (1, 1));
    }

    #[test]
    fn decodes_refinement_symbol_from_deltas() {
        let reference = JBig2Image::new(1, 1);
        let config = SymbolDictionaryRefinementConfig {
            template: RefinementTemplate::Template0,
            at: RefinementAdaptiveTemplate::default_for(RefinementTemplate::Template0),
        };
        let mut reader = pdf_utils::BitReader::new(&[0x00]);
        let mut decoder = JBig2ArithDecoder::new(&mut reader);

        let image = decode_refinement_symbol_from_deltas(
            &reference,
            0,
            0,
            0,
            0,
            "width",
            "height",
            config,
            |_, delta| Ok(delta),
            &mut decoder,
        )
        .expect("decode");

        assert_eq!((image.width(), image.height()), (1, 1));
    }
}
