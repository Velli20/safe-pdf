use crate::{
    error::Jbig2Error,
    image::JBig2Image,
    text_region::{
        refinement::{DecodedTextRegionInstance, TextRegionDecodeContext},
        state::TextRegionDecodeState,
    },
};

/// Shared refinement-image decode contract for JBIG2 text-region decoders.
///
/// Concrete drivers decode their codec-specific refinement payload into a
/// temporary bitmap, while the default draw path handles shared composition
/// and instance recording.
pub(crate) trait TextRegionRefinedInstanceDecodeDriver {
    /// Run a closure with simultaneous mutable access to both the shared
    /// decode context and the strip state.
    fn with_context_and_state<T>(
        &mut self,
        f: impl FnOnce(&mut TextRegionDecodeContext<'_>, &mut TextRegionDecodeState) -> T,
    ) -> T;

    /// Decode one refinement-coded symbol instance into a temporary bitmap.
    fn decode_refined_instance_image(
        &mut self,
        instance: DecodedTextRegionInstance,
    ) -> Result<JBig2Image, Jbig2Error>;

    /// Decode, render, and record one refinement-coded symbol instance.
    ///
    /// The default implementation shares the final composition path for both
    /// arithmetic and Huffman text regions after codec-specific refinement
    /// payload decoding.
    fn draw_refined_instance(
        &mut self,
        instance: DecodedTextRegionInstance,
    ) -> Result<(), Jbig2Error> {
        let image = self.decode_refined_instance_image(instance)?;
        self.with_context_and_state(|context, state| {
            context.draw_refined_image(state, instance, &image)
        })
        .map(|_| ())
    }
}

/// Shared strip-level driver for JBIG2 text-region symbol placement.
///
/// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 defines one strip loop that is
/// shared by both arithmetic and Huffman text-region decoding. Concrete
/// drivers implement the codec-specific bitstream reads, while this trait's
/// default methods handle the common strip progression, symbol-position
/// assembly, and refined/non-refined dispatch.
pub(crate) trait TextRegionStripDecodeDriver: TextRegionRefinedInstanceDecodeDriver {
    /// Return the shared text-region decode context used for parsed flags,
    /// symbol lookup, and final bitmap composition.
    fn context(&self) -> &TextRegionDecodeContext<'_>;

    /// Return the current text-region working state that tracks `STRIPT`,
    /// `CURS`, and the decoded instance count.
    fn state(&self) -> &TextRegionDecodeState;

    /// Return mutable access to the current text-region working state.
    fn state_mut(&mut self) -> &mut TextRegionDecodeState;

    /// Decode the next strip-header `DT` delta from the codec-specific stream.
    ///
    /// The default [`decode_next_strip_header`](Self::decode_next_strip_header)
    /// implementation scales this delta by `SBSTRIPS` and advances `STRIPT`.
    fn decode_next_strip_header_delta(&mut self) -> Result<i32, Jbig2Error>;

    /// Decode the first-symbol `DFS` delta for the current strip.
    ///
    /// This feeds the section 6.4.7 update that seeds `FIRSTS` and `CURS`
    /// when the next symbol is the first one in its strip.
    fn decode_first_symbol_delta(&mut self) -> Result<i32, Jbig2Error>;

    /// Decode the subsequent-symbol `DS` delta, or return `None` when the
    /// current strip terminates before another symbol instance.
    ///
    /// Huffman decoding uses out-of-band to signal strip termination; the
    /// arithmetic path uses its nullable integer decode for the same purpose.
    fn decode_delta_s_or_end(&mut self) -> Result<Option<i32>, Jbig2Error>;

    /// Decode the current-strip `T` offset for the next symbol instance.
    ///
    /// Callers should return `0` when `SBSTRIPS == 1` and no explicit
    /// strip-relative `T` value is present in the bitstream.
    fn decode_current_t(&mut self) -> Result<i64, Jbig2Error>;

    /// Decode the referenced symbol identifier for the next instance.
    fn decode_symbol_id(&mut self) -> Result<usize, Jbig2Error>;

    /// Decode whether the next symbol instance is refinement-coded.
    ///
    /// Drivers should return `false` when refinement is disabled by flags and
    /// the bitstream does not carry a refinement marker.
    fn decode_refinement_flag(&mut self) -> Result<bool, Jbig2Error>;

    /// Return whether all declared symbol instances have been decoded.
    ///
    /// This corresponds to the `NINSTANCES == SBNUMINSTANCES` termination
    /// check in section 6.4.5 step 3(a).
    fn is_complete(&self) -> bool {
        self.state()
            .is_complete(self.context().parsed().symbol_instances)
    }

    /// Decode the next strip header and return the updated absolute `STRIPT`.
    ///
    /// The default implementation decodes codec-specific `DT`, applies the
    /// `SBSTRIPS` scaling defined by section 6.4.6, and resets per-strip
    /// `CURS` state through [`TextRegionDecodeState::advance_strip`].
    fn decode_next_strip_header(&mut self) -> Result<i64, Jbig2Error> {
        let delta_t = self.decode_next_strip_header_delta()?;
        let sbstrips = self.context().parsed().flags.sbstrips();
        self.state_mut().advance_strip(delta_t, sbstrips)
    }

    /// Decode the next symbol position and metadata within the current strip.
    ///
    /// This default implementation performs the shared section 6.4.5 work:
    /// update `CURS` from either `DFS` or `DS`, compute `TI` from `STRIPT`
    /// plus the codec-specific `T`, decode the symbol ID, and attach the
    /// refinement flag.
    fn decode_strip_symbol_position(
        &mut self,
        stript: i64,
    ) -> Result<Option<DecodedTextRegionInstance>, Jbig2Error> {
        if self.state().strip_first_symbol_pending() {
            let delta_first_s = self.decode_first_symbol_delta()?;
            self.state_mut().consume_huffman_first_s(delta_first_s)?;
        } else {
            let Some(delta_s) = self.decode_delta_s_or_end()? else {
                return Ok(None);
            };
            let sbdsoffset = self.context().parsed().flags.sbdsoffset();
            self.state_mut()
                .consume_huffman_delta_s(delta_s, sbdsoffset)?;
        }

        let current_t = self.decode_current_t()?;
        let ti = self.context().instance_ti(stript, current_t)?;
        let symbol_id = self.decode_symbol_id()?;
        let refined = self.decode_refinement_flag()?;

        Ok(Some(DecodedTextRegionInstance {
            curs: self.state().strip_curs(),
            ti,
            symbol_id,
            refined,
        }))
    }

    /// Draw one decoded symbol instance into the region bitmap.
    ///
    /// Refined instances are delegated to
    /// [`draw_refined_instance`](Self::draw_refined_instance); all other
    /// instances use the shared non-refined composition path.
    fn draw_instance(&mut self, instance: DecodedTextRegionInstance) -> Result<(), Jbig2Error> {
        if instance.refined {
            return TextRegionRefinedInstanceDecodeDriver::draw_refined_instance(self, instance);
        }
        self.draw_non_refined_instance(instance)
    }

    /// Draw one non-refined symbol instance using the shared symbol bitmap.
    ///
    /// The default implementation composes the referenced dictionary symbol
    /// into the destination region and records the placed instance.
    fn draw_non_refined_instance(
        &mut self,
        instance: DecodedTextRegionInstance,
    ) -> Result<(), Jbig2Error> {
        self.with_context_and_state(|context, state| context.draw_decoded_symbol(state, instance))
            .map(|_| ())
    }
}

/// Decode all text-region strips until the declared symbol-instance count is
/// satisfied.
pub(crate) fn decode_text_region<D: TextRegionStripDecodeDriver>(
    driver: &mut D,
) -> Result<(), Jbig2Error> {
    while !driver.is_complete() {
        decode_text_region_strip(driver)?;
    }
    Ok(())
}

/// Decode one text-region strip using the shared strip-driver flow.
///
/// The driver first advances `STRIPT`, then repeatedly decodes and draws
/// symbol instances until the strip ends or all instances have been placed.
pub(crate) fn decode_text_region_strip<D: TextRegionStripDecodeDriver>(
    driver: &mut D,
) -> Result<(), Jbig2Error> {
    let stript = driver.decode_next_strip_header()?;

    while let Some(instance) = driver.decode_strip_symbol_position(stript)? {
        driver.draw_instance(instance)?;
        if driver.is_complete() {
            break;
        }
    }

    Ok(())
}
