//! JBIG2 Huffman tables and decoders.
//!
//! This module implements the standard Huffman table machinery from ITU-T
//! T.88 / ISO/IEC 14492 Annex B and the symbol-ID Huffman table used by the
//! text-region procedure. The ITU recommendation is available at
//! <https://www.itu.int/rec/T-REC-T.88>.

use crate::error::Jbig2Error;

mod code;
mod custom;
mod decoder;
mod selector;
mod standard;
mod symbol_id;
#[cfg(test)]
pub(crate) mod test_support;
mod tree;

pub(crate) use code::HuffmanCode;
pub(crate) use custom::CustomHuffmanDecoder;
pub(crate) use decoder::{HuffmanValue, StandardHuffmanDecoder};
pub(crate) use selector::{
    HuffmanTableSelection, text_region_refinement_standard_decoder,
    text_region_rsize_standard_decoder,
};
pub(crate) use standard::STANDARD_TABLE_B1;
#[cfg(test)]
pub(crate) use standard::{STANDARD_TABLE_B2, STANDARD_TABLE_B4};
pub(crate) use symbol_id::{
    SymbolIdHuffmanTable, decode_symbol_id, decode_symbol_id_huffman_table,
};

/// A JBIG2 Huffman decoder selected from either Annex B standard tables or a custom table segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HuffmanDecoder {
    /// Standard Annex B Huffman table.
    Standard(StandardHuffmanDecoder),
    /// Custom Huffman table decoded from a Tables segment.
    Custom(CustomHuffmanDecoder),
}

impl HuffmanDecoder {
    /// Decode one Huffman value from `reader`.
    pub(crate) fn decode(
        &self,
        reader: &mut pdf_utils::BitReader<'_>,
    ) -> Result<HuffmanValue, Jbig2Error> {
        match self {
            Self::Standard(decoder) => decoder.decode(reader),
            Self::Custom(decoder) => decoder.decode(reader),
        }
    }
}
