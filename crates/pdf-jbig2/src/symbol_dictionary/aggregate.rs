use crate::{
    compose_op::ComposeOp,
    error::Jbig2Error,
    image::JBig2Image,
    text_region::{
        geometry::TextRegionGeometry,
        state::{TextRegionDecodeState, advance_curs_by_delta_s},
    },
};

/// Drive the common symbol-instance placement loop for aggregate refinement.
///
/// Callers provide coding-method-specific readers for strip deltas, symbol IDs,
/// and symbol-instance resolution. The driver handles strip progression,
/// placement geometry, `ComposeOp::Or`, cursor advancement, and instance
/// counting.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_aggregate_symbol_instances<
    C,
    ReadDeltaT,
    ReadFirstS,
    ReadDeltaS,
    ReadSymbolId,
    ResolveSymbol,
>(
    context: &mut C,
    image: &mut JBig2Image,
    geometry: TextRegionGeometry,
    state: &mut TextRegionDecodeState,
    symbol_instances: u32,
    stop_before_delta_s_on_completion: bool,
    mut read_delta_t: ReadDeltaT,
    mut read_first_s_delta: ReadFirstS,
    mut read_delta_s: ReadDeltaS,
    mut read_symbol_id: ReadSymbolId,
    mut resolve_symbol: ResolveSymbol,
) -> Result<(), Jbig2Error>
where
    ReadDeltaT: FnMut(&mut C) -> Result<i32, Jbig2Error>,
    ReadFirstS: FnMut(&mut C) -> Result<i32, Jbig2Error>,
    ReadDeltaS: FnMut(&mut C) -> Result<Option<i32>, Jbig2Error>,
    ReadSymbolId: FnMut(&mut C) -> Result<usize, Jbig2Error>,
    ResolveSymbol: FnMut(&mut C, usize) -> Result<JBig2Image, Jbig2Error>,
{
    while !state.is_complete(symbol_instances) {
        let delta_t = read_delta_t(context)?;
        state.advance_strip(delta_t, 1)?;
        let delta_first_s = read_first_s_delta(context)?;
        let mut curs = state.consume_first_s_delta(delta_first_s)?;

        loop {
            let symbol_id = read_symbol_id(context)?;
            let symbol = resolve_symbol(context, symbol_id)?;
            let placed_curs =
                geometry.adjust_curs_before_placement(curs, symbol.width(), symbol.height())?;
            let placement = geometry.placement_for(
                placed_curs,
                state.stript(),
                symbol.width(),
                symbol.height(),
            )?;
            symbol.compose_clipped_to(image, placement.x, placement.y, ComposeOp::Or);
            curs = geometry.advance_curs_after_placement(
                placed_curs,
                symbol.width(),
                symbol.height(),
            )?;
            state.record_instance()?;

            if stop_before_delta_s_on_completion && state.is_complete(symbol_instances) {
                break;
            }
            let Some(delta_s) = read_delta_s(context)? else {
                break;
            };
            curs = advance_curs_by_delta_s(curs, delta_s, 0)?;
            if state.is_complete(symbol_instances) {
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::decode_aggregate_symbol_instances;
    use crate::text_region::{geometry::TextRegionRefCorner, state::TextRegionDecodeState};
    use crate::{error::Jbig2Error, image::JBig2Image, text_region::geometry::TextRegionGeometry};

    #[test]
    fn composes_multi_instance_symbols_with_cursor_progression() -> Result<(), Jbig2Error> {
        let mut image = JBig2Image::try_new(2, 1, None)?;
        let mut first = JBig2Image::try_new(1, 1, None)?;
        first.set_pixel(0, 0, 1);
        let mut second = JBig2Image::try_new(1, 1, None)?;
        second.set_pixel(0, 0, 1);
        let symbols = [first, second];
        let geometry = TextRegionGeometry::new(false, TextRegionRefCorner::TopLeft);
        let mut state = TextRegionDecodeState::from_initial_delta(0, 1)?;
        let mut context = ();
        let mut read_delta_t = Some(0);
        let mut read_first_s = Some(0);
        let mut next_symbol_id = 0usize;
        let mut next_delta_s = Some(1);

        decode_aggregate_symbol_instances(
            &mut context,
            &mut image,
            geometry,
            &mut state,
            2,
            false,
            |_| {
                read_delta_t
                    .take()
                    .ok_or(Jbig2Error::InvalidState("delta t"))
            },
            |_| {
                read_first_s
                    .take()
                    .ok_or(Jbig2Error::InvalidState("first s"))
            },
            |_| {
                let value = next_delta_s;
                next_delta_s = None;
                Ok(value)
            },
            |_| {
                let value = next_symbol_id;
                next_symbol_id = next_symbol_id
                    .checked_add(1)
                    .ok_or(Jbig2Error::Overflow("symbol id"))?;
                Ok(value)
            },
            |_, symbol_id| {
                symbols
                    .get(symbol_id)
                    .cloned()
                    .ok_or(Jbig2Error::InvalidState("symbol"))
            },
        )?;

        assert_eq!(image.pixel_at_offset(0, 0, 0, 0), 1);
        assert_eq!(image.pixel_at_offset(1, 0, 0, 0), 1);
        Ok(())
    }
}
