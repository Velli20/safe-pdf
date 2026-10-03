//! Failures raised by the shared MQ arithmetic decoder.

use thiserror::Error;

/// A failure while decoding an MQ arithmetic decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum MqError {
    /// Decoding was initialized without a single arithmetic byte.
    #[error("arithmetic stream is truncated")]
    Truncated,
    /// A context named a probability state outside the estimation table.
    #[error("arithmetic probability state is invalid")]
    InvalidState,
}
