//! JBIG2 symbol dictionary segment decoding.
//!
//! ITU-T T.88 / ISO/IEC 14492 section 7.4.2 defines symbol dictionary
//! segments. They produce reusable symbol bitmaps for text-region segments.

mod aggregate;
mod arithmetic;
mod collective_bitmap;
mod current_symbol_set;
mod export;
mod flags;
mod header;
mod huffman;
mod refinement;

use self::{
    arithmetic::decode_arithmetic_symbol_dictionary, flags::SymbolDictionaryFlagBits,
    header::ParsedSymbolDictionaryHeader, huffman::HuffmanSymbolDictionaryDecoder,
};
use crate::{
    error::Jbig2Error, image::JBig2Image, segment_context::SegmentDecodeContext,
    symbol_dictionary::export::export_dictionary_symbols,
};

/// Decoded JBIG2 symbol dictionary segment.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 defines symbol dictionaries as
/// ordered sets of symbol bitmaps. Text regions reference these images by
/// symbol index during the section 6.4 text-region decoding procedure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct SymbolDictionary {
    /// Exported symbol images in symbol dictionary index order.
    pub(crate) images: Vec<JBig2Image>,
}

impl SymbolDictionary {
    /// Decode one JBIG2 symbol dictionary segment from `context`.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2.1 defines the symbol
    /// dictionary header. The `SDHUFF` flag selects the Huffman procedure;
    /// otherwise the arithmetic procedure is used.
    pub(crate) fn from_reader(
        context: &mut SegmentDecodeContext<'_, '_, '_, '_, '_>,
    ) -> Result<SymbolDictionary, Jbig2Error> {
        SymbolDictionaryDecoder::new(context)?.decode()
    }

    /// Build the exported dictionary from referred and newly decoded symbols.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 defines export flags over the
    /// concatenation of referred input symbols and newly decoded symbols. This
    /// helper applies those flags and returns the exported symbol images.
    fn from_export_flags(
        input_symbols: &[JBig2Image],
        new_symbols: &[JBig2Image],
        export_flags: &[bool],
        num_exported: usize,
    ) -> Result<Self, Jbig2Error> {
        export_dictionary_symbols(input_symbols, new_symbols, export_flags, num_exported)
            .map(|images| Self { images })
    }
}

/// Segment-local decoder state for one symbol dictionary.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 combines the parsed header,
/// referred input symbols, and remaining segment body when decoding the final
/// exported symbol dictionary.
struct SymbolDictionaryDecoder<'context, 'segment, 'stream, 'data, 'decoded, 'prior> {
    header: ParsedSymbolDictionaryHeader,
    context: &'context mut SegmentDecodeContext<'segment, 'stream, 'data, 'decoded, 'prior>,
    input_symbols: Vec<JBig2Image>,
}

impl<'context, 'segment, 'stream, 'data, 'decoded, 'prior>
    SymbolDictionaryDecoder<'context, 'segment, 'stream, 'data, 'decoded, 'prior>
{
    /// Create a decoder by parsing the symbol dictionary header and inputs.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2.1 places the header at the
    /// start of the segment body. Referred symbol dictionaries contribute the
    /// input symbols used by section 7.4.2 export selection.
    fn new(
        context: &'context mut SegmentDecodeContext<'segment, 'stream, 'data, 'decoded, 'prior>,
    ) -> Result<Self, Jbig2Error> {
        let header = ParsedSymbolDictionaryHeader::try_from(&mut *context.stream())?;
        let input_symbols = context.referred_symbol_images()?;
        Ok(Self {
            header,
            context,
            input_symbols,
        })
    }

    /// Decode the symbol dictionary using the coding method selected by `SDHUFF`.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2.1.1 defines `SDHUFF`; a set bit
    /// selects Huffman decoding and a clear bit selects arithmetic decoding.
    fn decode(mut self) -> Result<SymbolDictionary, Jbig2Error> {
        if self.header.flags.contains(SymbolDictionaryFlagBits::SDHUFF) {
            self.decode_huffman_dictionary()
        } else {
            self.decode_arithmetic_dictionary()
        }
    }

    /// Decode a Huffman-coded symbol dictionary body.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 uses standard Huffman tables
    /// selected by the symbol dictionary flags to decode heights, widths,
    /// collective bitmap sizes, and export runs.
    fn decode_huffman_dictionary(&mut self) -> Result<SymbolDictionary, Jbig2Error> {
        let custom_tables = self.referred_huffman_tables()?;
        let mut decoder =
            HuffmanSymbolDictionaryDecoder::new(self.context.stream(), self.header, custom_tables)?;
        let new_symbols =
            decoder.decode_new_symbols(&self.input_symbols, self.header.num_new_symbols)?;
        let export_flags =
            decoder.decode_export_flags(self.input_symbols.len(), new_symbols.len())?;
        let dictionary = SymbolDictionary::from_export_flags(
            &self.input_symbols,
            &new_symbols,
            &export_flags,
            self.header.num_exported,
        )?;
        Ok(dictionary)
    }

    fn referred_huffman_tables(
        &self,
    ) -> Result<Vec<crate::huffman::CustomHuffmanDecoder>, Jbig2Error> {
        let mut tables = Vec::new();
        for index in 0usize.. {
            match self.context.referred_huffman_table(index) {
                Ok(table) => tables.push(table.clone()),
                Err(Jbig2Error::MissingSegment) => break,
                Err(err) => return Err(err),
            }
        }
        Ok(tables)
    }

    /// Decode an arithmetic-coded symbol dictionary body.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 uses arithmetic integer
    /// contexts for symbol height deltas, width deltas, and export run lengths.
    fn decode_arithmetic_dictionary(&mut self) -> Result<SymbolDictionary, Jbig2Error> {
        let mut decoder = self.context.arithmetic_decoder_until_end()?;
        let new_symbols =
            decode_arithmetic_symbol_dictionary(&self.header, &self.input_symbols, &mut decoder)?;
        let export_flags = arithmetic::decode_export_flags(
            &mut decoder,
            self.input_symbols.len(),
            new_symbols.len(),
        )?;

        SymbolDictionary::from_export_flags(
            &self.input_symbols,
            &new_symbols,
            &export_flags,
            self.header.num_exported,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SymbolDictionary;
    use crate::{error::Jbig2Error, image::JBig2Image};

    #[test]
    fn exported_dictionary_combines_input_and_new_symbols() {
        let input_symbols = vec![JBig2Image::new(1, 1), JBig2Image::new(2, 1)];
        let new_symbols = vec![JBig2Image::new(3, 1), JBig2Image::new(4, 1)];
        let export_flags = [true, false, true, true];

        let dictionary =
            SymbolDictionary::from_export_flags(&input_symbols, &new_symbols, &export_flags, 3)
                .expect("dictionary");

        assert_eq!(dictionary.images.len(), 3);
        assert_eq!(dictionary.images.first().expect("first").width(), 1);
        assert_eq!(dictionary.images.get(1).expect("second").width(), 3);
        assert_eq!(dictionary.images.get(2).expect("third").width(), 4);
    }

    #[test]
    fn exported_dictionary_rejects_extra_exported_symbols() {
        let err = SymbolDictionary::from_export_flags(&[], &[], &[true, true], 1)
            .expect_err("exported count error");
        assert_eq!(err, Jbig2Error::InvalidState("exported symbol count"));
    }

    #[test]
    fn exported_dictionary_rejects_missing_decoded_symbol() {
        let err = SymbolDictionary::from_export_flags(&[], &[], &[true], 1)
            .expect_err("missing symbol error");
        assert_eq!(err, Jbig2Error::MissingSymbol("decoded"));
    }
}
