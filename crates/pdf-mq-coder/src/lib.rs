//! The MQ arithmetic decoder shared by JBIG2 and JPEG 2000.
//!
//! ITU-T T.88 Annex A and ITU-T T.800 Annex C specify the same binary
//! arithmetic coder: one 47-entry probability-estimation table, one pair of
//! interval and code registers, and the `DECODE`, `MPS_EXCHANGE`,
//! `LPS_EXCHANGE`, `RENORMD`, and `BYTEIN` procedures. Only the meaning of a
//! context differs between the two standards, so this crate owns the coder and
//! each decoder owns its own contexts.
//!
//! The registers are kept in the complemented "software conventions" form: the
//! code register is compared against the interval register directly and
//! `BYTEIN` subtracts the incoming byte. That is the formulation both standards
//! describe as an equivalent implementation, and it avoids a separate
//! high-order register.
//!
//! Bytes arrive through [`MqByteSource`] rather than a slice, because JBIG2
//! reads them from a segment inside a larger stream while JPEG 2000 reads a
//! self-contained codeword segment.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod byte_source;
mod context;
mod error;
mod probability;
mod registers;

pub use byte_source::{MqByteSource, SliceSource};
pub use context::MqContext;
pub use error::MqError;
pub use registers::MqRegisters;
