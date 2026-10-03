mod bit_reader;
mod context;
mod decoder;
mod segment_source;

pub(crate) mod generic_region;
pub(crate) mod iaid;
pub(crate) mod integer;
pub(crate) mod template_refs;

pub(crate) use decoder::JBig2ArithDecoder;
pub(crate) use integer::JBig2ArithIntegerContext;
