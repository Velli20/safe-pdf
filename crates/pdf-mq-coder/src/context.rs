//! One adaptive arithmetic context.
//!
//! A context is the entire adaptive state of the MQ coder: the symbol it
//! currently considers more probable, and an index into the probability
//! estimation table. How many contexts exist and what each one models is left
//! to the standard using the coder, so this crate stores one and no more.

use crate::probability::ProbabilityState;

/// The adaptive state of one MQ arithmetic context.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MqContext {
    mps: bool,
    index: u8,
}

impl MqContext {
    /// Creates a context starting at a given probability state and symbol.
    ///
    /// JPEG 2000 Annex D starts three of its contexts away from state zero;
    /// JBIG2 starts every context at the default.
    pub fn with_state(index: u8, mps: bool) -> Self {
        Self { mps, index }
    }

    /// Applies the less-probable transition and returns the decoded bit.
    pub(crate) fn decode_nlps(&mut self, state: &ProbabilityState) -> bool {
        let decoded = !self.mps;
        if state.switch_mps {
            self.mps = !self.mps;
        }
        self.index = state.nlps;
        decoded
    }

    /// Applies the more-probable transition and returns the decoded bit.
    pub(crate) fn decode_nmps(&mut self, state: &ProbabilityState) -> bool {
        self.index = state.nmps;
        self.mps
    }

    /// Returns the context's index into the probability estimation table.
    pub(crate) fn probability_index(self) -> u8 {
        self.index
    }

    /// Returns the symbol the context currently considers more probable.
    pub(crate) fn mps(self) -> bool {
        self.mps
    }
}

#[cfg(test)]
mod tests {
    use super::MqContext;
    use crate::probability::ProbabilityState;

    #[test]
    fn lps_transition_switches_mps_when_probability_state_requires_it() {
        let state = ProbabilityState {
            qe: 0x5601,
            nmps: 1,
            nlps: 1,
            switch_mps: true,
        };
        let mut context = MqContext::default();

        assert!(context.decode_nlps(&state));
        assert!(context.mps());
        assert_eq!(context.probability_index(), 1);
    }

    #[test]
    fn mps_transition_preserves_symbol_and_updates_probability_index() {
        let state = ProbabilityState {
            qe: 0x3401,
            nmps: 2,
            nlps: 6,
            switch_mps: false,
        };
        let mut context = MqContext::default();

        assert!(!context.decode_nmps(&state));
        assert!(!context.mps());
        assert_eq!(context.probability_index(), 2);
    }
}
