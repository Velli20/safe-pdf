//! The nineteen arithmetic contexts of the code-block entropy coder.
//!
//! Annex D gives the MQ coder a fixed set of contexts: nine for significance,
//! five for sign, three for magnitude refinement, one run-length context, and
//! one uniform context. Table D.7 starts three of them away from probability
//! state zero, which is the only initialization the standard prescribes.
//!
//! The `RESET` code-block style returns every context to that initial state
//! before each coding pass, so the set is reset rather than rebuilt.

use pdf_mq_coder::MqContext;

/// Contexts defined by Annex D for one code-block.
const CONTEXT_COUNT: usize = 19;
/// Label of the run-length context used by the cleanup pass.
const RUN_LENGTH_LABEL: usize = 17;
/// Label of the uniform context, which never adapts far from equiprobable.
const UNIFORM_LABEL: usize = 18;
/// Initial probability state of the all-insignificant significance context.
const INITIAL_SIGNIFICANCE_STATE: u8 = 4;
/// Initial probability state of the run-length context.
const INITIAL_RUN_LENGTH_STATE: u8 = 3;
/// Initial probability state of the uniform context.
const INITIAL_UNIFORM_STATE: u8 = 46;

/// Identifies one of the nineteen Annex D contexts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContextLabel(usize);

impl ContextLabel {
    /// The run-length context of the cleanup pass.
    pub(crate) const RUN_LENGTH: Self = Self(RUN_LENGTH_LABEL);
    /// The uniform context, used for run-length positions and segment symbols.
    pub(crate) const UNIFORM: Self = Self(UNIFORM_LABEL);

    /// Returns the label with the given index, if Annex D defines one.
    pub(crate) fn new(label: usize) -> Option<Self> {
        (label < CONTEXT_COUNT).then_some(Self(label))
    }
}

/// The adaptive state of every Annex D context of one code-block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContextSet {
    contexts: [MqContext; CONTEXT_COUNT],
}

impl Default for ContextSet {
    fn default() -> Self {
        let mut contexts = [MqContext::default(); CONTEXT_COUNT];
        let initial = [
            (0, INITIAL_SIGNIFICANCE_STATE),
            (RUN_LENGTH_LABEL, INITIAL_RUN_LENGTH_STATE),
            (UNIFORM_LABEL, INITIAL_UNIFORM_STATE),
        ];
        for (label, state) in initial {
            if let Some(context) = contexts.get_mut(label) {
                *context = MqContext::with_state(state, false);
            }
        }
        Self { contexts }
    }
}

impl ContextSet {
    /// Returns every context to its Table D.7 initial state.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Returns one context for decoding a decision.
    pub(crate) fn context(&mut self, label: ContextLabel) -> Option<&mut MqContext> {
        self.contexts.get_mut(label.0)
    }
}
