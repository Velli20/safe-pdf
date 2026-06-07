//! Core JBIG2 arithmetic coding step.
//!
//! This module implements the ITU-T T.88 / ISO/IEC 14492 Annex A.1 decision
//! decoding and renormalization procedures. Context-pool ownership and higher
//! level integer/IAID procedures live in separate modules.

use crate::arith_decoder::{
    context::JBig2ArithCtx, decoder::JBig2ArithDecoder, probability::QE_TABLE,
};
use crate::error::Jbig2Error;

/// Initial and renormalization interval high bit from T.88 Annex A.1.
pub(super) const DEFAULT_INTERVAL: u32 = 0x8000;

/// Initial code-register shift from T.88 Annex A.1 decoder initialization.
pub(super) const INITIAL_CODE_REGISTER_SHIFT: u32 = 16;

/// Final initialization shift after the first Annex A.1 `BYTEIN`.
pub(super) const POST_BYTE_IN_CODE_SHIFT: u32 = 7;

impl JBig2ArithDecoder<'_, '_> {
    /// Decode one binary decision using the Annex A.1 arithmetic procedure.
    pub(super) fn decode(&mut self, ctx: &mut JBig2ArithCtx) -> Result<u8, Jbig2Error> {
        if self.complete {
            return Err(Jbig2Error::Truncated("arithmetic stream"));
        }

        let Some(state) = QE_TABLE.get(usize::from(ctx.probability_index())) else {
            return Err(Jbig2Error::InvalidState("arithmetic probability state"));
        };

        self.interval = self.interval.wrapping_sub(state.qe);
        if (self.code >> INITIAL_CODE_REGISTER_SHIFT) < self.interval {
            if (self.interval & DEFAULT_INTERVAL) != 0 {
                return Ok(u8::from(ctx.mps()));
            }

            let decoded = if self.interval < state.qe {
                ctx.decode_nlps(state)
            } else {
                ctx.decode_nmps(state)
            };
            self.renormalize();
            return Ok(decoded);
        }

        self.code = self
            .code
            .wrapping_sub(self.interval << INITIAL_CODE_REGISTER_SHIFT);
        let decoded = if self.interval < state.qe {
            ctx.decode_nmps(state)
        } else {
            ctx.decode_nlps(state)
        };
        self.interval = state.qe;
        self.renormalize();
        Ok(decoded)
    }

    /// Renormalize the interval and code registers per T.88 Annex A.1.
    fn renormalize(&mut self) {
        while (self.interval & DEFAULT_INTERVAL) == 0 {
            if self.bit_count == 0 {
                self.byte_in();
            }
            self.interval = self.interval.wrapping_shl(1);
            self.code = self.code.wrapping_shl(1);
            self.bit_count = self.bit_count.saturating_sub(1);
        }
    }
}
