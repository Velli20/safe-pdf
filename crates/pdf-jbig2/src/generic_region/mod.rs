//! JBIG2 generic-region parsing and arithmetic/MMR decoding.

mod adaptive_template;
mod arithmetic;
mod flags;
mod mmr;
mod optimized;
mod parser;
pub(crate) mod tables;

pub(crate) use adaptive_template::GenericRegionAdaptiveTemplate;
pub(crate) use flags::{GenericRegionFlags, GenericRegionTemplate};
pub(crate) use mmr::decode_mmr_region;
pub(crate) use parser::GenericRegion;
