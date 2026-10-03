//! Arithmetic probability-context state and context pools.
//!
//! One adaptive context is a probability state shared with JPEG 2000 and kept
//! by `pdf-mq-coder`. What JBIG2 adds is how many of them exist and which one
//! a decision uses, so this module owns the separate context pools for generic
//! regions, Annex A.2 arithmetic integers, and Annex A.3 IAID values.

use crate::arith_decoder::{
    bit_reader::BitContextSource,
    decoder::JBig2ArithDecoder,
    integer::{ARITH_INT_CONTEXT_COUNT, JBig2ArithIntegerContext},
};
use crate::error::Jbig2Error;
use crate::generic_region::tables::CONTEXT_COUNT as GENERIC_REGION_CONTEXT_COUNT;
use pdf_mq_coder::MqContext;

/// Identifies an already-initialized context pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PoolSelector {
    /// Generic-region bitmap contexts from T.88 section 6.2.5.7.
    Generic,
    /// Arithmetic integer contexts from T.88 Annex A.2.
    Integer(JBig2ArithIntegerContext),
    /// IAID contexts from T.88 Annex A.3.
    Iaid,
}

impl JBig2ArithDecoder<'_, '_> {
    /// Decode one bit from the selected T.88 arithmetic context pool.
    pub(super) fn decode_selected_context(
        &mut self,
        source: &BitContextSource,
        index: usize,
    ) -> Result<u8, Jbig2Error> {
        match source {
            BitContextSource::Integer(context) => {
                let contexts = self.integer_context_pool_mut(context);
                Self::ensure_contexts(
                    contexts,
                    ARITH_INT_CONTEXT_COUNT,
                    "arithmetic integer contexts",
                )?;
                self.decode_context_from_pool(&PoolSelector::Integer(*context), index)
            }
            BitContextSource::Iaid => self.decode_context_from_pool(&PoolSelector::Iaid, index),
        }
    }

    /// Ensure the generic-region context pool exists before a pixel loop.
    pub(crate) fn ensure_generic_region_contexts(&mut self) -> Result<(), Jbig2Error> {
        Self::ensure_contexts(
            &mut self.generic_region_contexts,
            GENERIC_REGION_CONTEXT_COUNT,
            "generic region contexts",
        )
    }

    /// Decode one bit from an already-initialized generic-region context.
    ///
    /// Generic-region pixel loops call this after [`Self::ensure_generic_region_contexts`]
    /// so the hot path does not repeatedly dispatch on context-pool type or
    /// check whether the pool needs allocation.
    pub(crate) fn decode_prepared_generic_context(
        &mut self,
        index: usize,
    ) -> Result<u8, Jbig2Error> {
        self.decode_context_from_pool(&PoolSelector::Generic, index)
    }

    /// Decode one bit from an initialized context pool and write back its state.
    fn decode_context_from_pool(
        &mut self,
        pool: &PoolSelector,
        index: usize,
    ) -> Result<u8, Jbig2Error> {
        let mut ctx = {
            let contexts = self.contexts(pool);
            *contexts
                .get(index)
                .ok_or(Jbig2Error::Overflow("arithmetic context overflow"))?
        };
        let decoded = self.decode(&mut ctx)?;
        let slot = self
            .contexts_mut(pool)
            .get_mut(index)
            .ok_or(Jbig2Error::Overflow("arithmetic context overflow"))?;
        *slot = ctx;
        Ok(decoded)
    }

    /// Return the mutable Annex A.2 context pool for an integer class.
    fn integer_context_pool_mut(
        &mut self,
        context: &JBig2ArithIntegerContext,
    ) -> &mut Vec<MqContext> {
        match context {
            JBig2ArithIntegerContext::TextDeltaT => &mut self.iadt_contexts,
            JBig2ArithIntegerContext::TextFirstS => &mut self.iafs_contexts,
            JBig2ArithIntegerContext::TextInstanceT => &mut self.iait_contexts,
            JBig2ArithIntegerContext::TextDeltaS => &mut self.iads_contexts,
            JBig2ArithIntegerContext::SymbolHeightDelta => &mut self.iadh_contexts,
            JBig2ArithIntegerContext::SymbolWidthDelta => &mut self.iadw_contexts,
            JBig2ArithIntegerContext::SymbolExportRunLength => &mut self.iaex_contexts,
            JBig2ArithIntegerContext::RefinementAggregateInstances => &mut self.iaai_contexts,
            JBig2ArithIntegerContext::RefinementDeltaWidth => &mut self.iardw_contexts,
            JBig2ArithIntegerContext::RefinementDeltaHeight => &mut self.iardh_contexts,
            JBig2ArithIntegerContext::RefinementDeltaX => &mut self.iardx_contexts,
            JBig2ArithIntegerContext::RefinementDeltaY => &mut self.iardy_contexts,
            JBig2ArithIntegerContext::RefinementInstance => &mut self.iari_contexts,
        }
    }

    /// Return an initialized arithmetic context pool by selector.
    fn contexts(&self, pool: &PoolSelector) -> &[MqContext] {
        match pool {
            PoolSelector::Generic => &self.generic_region_contexts,
            PoolSelector::Integer(context) => match context {
                JBig2ArithIntegerContext::TextDeltaT => &self.iadt_contexts,
                JBig2ArithIntegerContext::TextFirstS => &self.iafs_contexts,
                JBig2ArithIntegerContext::TextInstanceT => &self.iait_contexts,
                JBig2ArithIntegerContext::TextDeltaS => &self.iads_contexts,
                JBig2ArithIntegerContext::SymbolHeightDelta => &self.iadh_contexts,
                JBig2ArithIntegerContext::SymbolWidthDelta => &self.iadw_contexts,
                JBig2ArithIntegerContext::SymbolExportRunLength => &self.iaex_contexts,
                JBig2ArithIntegerContext::RefinementAggregateInstances => &self.iaai_contexts,
                JBig2ArithIntegerContext::RefinementDeltaWidth => &self.iardw_contexts,
                JBig2ArithIntegerContext::RefinementDeltaHeight => &self.iardh_contexts,
                JBig2ArithIntegerContext::RefinementDeltaX => &self.iardx_contexts,
                JBig2ArithIntegerContext::RefinementDeltaY => &self.iardy_contexts,
                JBig2ArithIntegerContext::RefinementInstance => &self.iari_contexts,
            },
            PoolSelector::Iaid => &self.iaid_contexts,
        }
    }

    /// Return a mutable initialized arithmetic context pool by selector.
    fn contexts_mut(&mut self, pool: &PoolSelector) -> &mut Vec<MqContext> {
        match pool {
            PoolSelector::Generic => &mut self.generic_region_contexts,
            PoolSelector::Integer(context) => match context {
                JBig2ArithIntegerContext::TextDeltaT => &mut self.iadt_contexts,
                JBig2ArithIntegerContext::TextFirstS => &mut self.iafs_contexts,
                JBig2ArithIntegerContext::TextInstanceT => &mut self.iait_contexts,
                JBig2ArithIntegerContext::TextDeltaS => &mut self.iads_contexts,
                JBig2ArithIntegerContext::SymbolHeightDelta => &mut self.iadh_contexts,
                JBig2ArithIntegerContext::SymbolWidthDelta => &mut self.iadw_contexts,
                JBig2ArithIntegerContext::SymbolExportRunLength => &mut self.iaex_contexts,
                JBig2ArithIntegerContext::RefinementAggregateInstances => &mut self.iaai_contexts,
                JBig2ArithIntegerContext::RefinementDeltaWidth => &mut self.iardw_contexts,
                JBig2ArithIntegerContext::RefinementDeltaHeight => &mut self.iardh_contexts,
                JBig2ArithIntegerContext::RefinementDeltaX => &mut self.iardx_contexts,
                JBig2ArithIntegerContext::RefinementDeltaY => &mut self.iardy_contexts,
                JBig2ArithIntegerContext::RefinementInstance => &mut self.iari_contexts,
            },
            PoolSelector::Iaid => &mut self.iaid_contexts,
        }
    }

    /// Allocate or reset a context pool to the size required by T.88.
    pub(super) fn ensure_contexts(
        contexts: &mut Vec<MqContext>,
        len: usize,
        label: &'static str,
    ) -> Result<(), Jbig2Error> {
        if contexts.len() == len {
            return Ok(());
        }
        contexts.clear();
        contexts
            .try_reserve_exact(len)
            .map_err(|_| Jbig2Error::Allocation(label))?;
        contexts.resize(len, MqContext::default());
        Ok(())
    }
}
