//! Probability estimation table for JBIG2 arithmetic coding.
//!
//! ITU-T T.88 / ISO/IEC 14492 Annex A.1 defines the `QE`, `NMPS`, `NLPS`, and
//! MPS-switch entries used by every arithmetic context.

/// One T.88 Annex A.1 probability-estimation state.
#[derive(Debug, Clone, Copy)]
pub(super) struct ProbabilityState {
    /// Probability interval subdivision value `Qe`.
    pub(super) qe: u32,
    /// Next state when the MPS path is decoded.
    pub(super) nmps: u8,
    /// Next state when the LPS path is decoded.
    pub(super) nlps: u8,
    /// Whether the LPS transition toggles the context MPS bit.
    pub(super) switch_mps: bool,
}

/// T.88 Annex A.1 arithmetic probability-estimation table.
pub(super) const QE_TABLE: [ProbabilityState; 47] = [
    ProbabilityState {
        qe: 0x5601,
        nmps: 1,
        nlps: 1,
        switch_mps: true,
    },
    ProbabilityState {
        qe: 0x3401,
        nmps: 2,
        nlps: 6,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x1801,
        nmps: 3,
        nlps: 9,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0ac1,
        nmps: 4,
        nlps: 12,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0521,
        nmps: 5,
        nlps: 29,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0221,
        nmps: 38,
        nlps: 33,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x5601,
        nmps: 7,
        nlps: 6,
        switch_mps: true,
    },
    ProbabilityState {
        qe: 0x5401,
        nmps: 8,
        nlps: 14,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x4801,
        nmps: 9,
        nlps: 14,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x3801,
        nmps: 10,
        nlps: 14,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x3001,
        nmps: 11,
        nlps: 17,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x2401,
        nmps: 12,
        nlps: 18,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x1c01,
        nmps: 13,
        nlps: 20,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x1601,
        nmps: 29,
        nlps: 21,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x5601,
        nmps: 15,
        nlps: 14,
        switch_mps: true,
    },
    ProbabilityState {
        qe: 0x5401,
        nmps: 16,
        nlps: 14,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x5101,
        nmps: 17,
        nlps: 15,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x4801,
        nmps: 18,
        nlps: 16,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x3801,
        nmps: 19,
        nlps: 17,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x3401,
        nmps: 20,
        nlps: 18,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x3001,
        nmps: 21,
        nlps: 19,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x2801,
        nmps: 22,
        nlps: 19,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x2401,
        nmps: 23,
        nlps: 20,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x2201,
        nmps: 24,
        nlps: 21,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x1c01,
        nmps: 25,
        nlps: 22,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x1801,
        nmps: 26,
        nlps: 23,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x1601,
        nmps: 27,
        nlps: 24,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x1401,
        nmps: 28,
        nlps: 25,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x1201,
        nmps: 29,
        nlps: 26,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x1101,
        nmps: 30,
        nlps: 27,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0ac1,
        nmps: 31,
        nlps: 28,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x09c1,
        nmps: 32,
        nlps: 29,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x08a1,
        nmps: 33,
        nlps: 30,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0521,
        nmps: 34,
        nlps: 31,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0441,
        nmps: 35,
        nlps: 32,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x02a1,
        nmps: 36,
        nlps: 33,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0221,
        nmps: 37,
        nlps: 34,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0141,
        nmps: 38,
        nlps: 35,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0111,
        nmps: 39,
        nlps: 36,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0085,
        nmps: 40,
        nlps: 37,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0049,
        nmps: 41,
        nlps: 38,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0025,
        nmps: 42,
        nlps: 39,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0015,
        nmps: 43,
        nlps: 40,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0009,
        nmps: 44,
        nlps: 41,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0005,
        nmps: 45,
        nlps: 42,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x0001,
        nmps: 45,
        nlps: 43,
        switch_mps: false,
    },
    ProbabilityState {
        qe: 0x5601,
        nmps: 46,
        nlps: 46,
        switch_mps: false,
    },
];
