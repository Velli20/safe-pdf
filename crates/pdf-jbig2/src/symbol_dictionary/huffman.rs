use pdf_utils::BitReader;

use crate::{
    error::Jbig2Error,
    generic_region::decode_mmr_region,
    huffman::{
        CustomHuffmanDecoder, HuffmanDecoder, HuffmanTableSelection, HuffmanValue,
        STANDARD_TABLE_B1, StandardHuffmanDecoder,
    },
    image::JBig2Image,
    symbol_dictionary::{
        collective_bitmap::append_collective_bitmap_symbols,
        export::{fill_export_flag_run, total_symbol_count},
        flags::SymbolDictionaryFlagBits,
    },
    util::{i32_to_u16, i32_to_usize, required_huffman_value, usize_to_u16},
};

const COLLECTIVE_BITMAP: &str = "collective bitmap";
const COLLECTIVE_BITMAP_SIZE: &str = "collective bitmap size";
const COLLECTIVE_BITMAP_WIDTH: &str = "collective bitmap width";
const HUFFMAN_SYMBOL_HEIGHT: &str = "Huffman symbol height";
const HUFFMAN_SYMBOL_WIDTH: &str = "Huffman symbol width";
const IMAGE_DIMENSIONS_OVERFLOW: &str = "image dimensions overflow";
const INTEGER_CONVERSION_OVERFLOW: &str = "integer conversion overflow";
const SYMBOL_DICTIONARY_WIDTH_RUN: &str = "symbol dictionary width run";

/// Huffman-coded symbol dictionary decoder.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 uses the `SDHUFFDH`,
/// `SDHUFFDW`, and `SDHUFFBMSIZE` selectors to choose standard Huffman tables
/// from Annex B for Huffman-coded symbol dictionaries.
pub(super) struct HuffmanSymbolDictionaryDecoder<'stream, 'data> {
    stream: &'stream mut BitReader<'data>,
    dh_table: HuffmanDecoder,
    dw_table: HuffmanDecoder,
    bmsize_table: HuffmanDecoder,
    export_table: StandardHuffmanDecoder,
}

impl<'stream, 'data> HuffmanSymbolDictionaryDecoder<'stream, 'data> {
    /// Build decoders for the standard Huffman tables selected by the header flags.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2.1.1 defines the table selector
    /// fields. This implementation supports only the standard Annex B tables.
    pub(super) fn new(
        stream: &'stream mut BitReader<'data>,
        flags: SymbolDictionaryFlagBits,
        custom_tables: &[CustomHuffmanDecoder],
    ) -> Result<Self, Jbig2Error> {
        let mut custom_index = 0usize;
        let dh_table = symbol_dictionary_table(
            HuffmanTableSelection::SymbolDictionaryDh(flags.sdhuffdh()),
            custom_tables,
            &mut custom_index,
        )?;
        let dw_table = symbol_dictionary_table(
            HuffmanTableSelection::SymbolDictionaryDw(flags.sdhuffdw()),
            custom_tables,
            &mut custom_index,
        )?;
        let bmsize_table = if flags.sdhuffbmsize() {
            next_custom_table(custom_tables, &mut custom_index)?
        } else {
            HuffmanDecoder::Standard(StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?)
        };
        let export_table = StandardHuffmanDecoder::new(STANDARD_TABLE_B1)?;

        Ok(Self {
            stream,
            dh_table,
            dw_table,
            bmsize_table,
            export_table,
        })
    }

    /// Decode all new symbols declared by a Huffman-coded symbol dictionary.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 groups Huffman-coded symbols
    /// by height. Each height run carries width deltas followed by a
    /// horizontally concatenated collective bitmap for that run.
    pub(super) fn decode_new_symbols(
        &mut self,
        num_new_symbols: usize,
    ) -> Result<Vec<JBig2Image>, Jbig2Error> {
        let mut new_symbols = Vec::with_capacity(num_new_symbols);
        let mut height = 0i32;

        while new_symbols.len() < num_new_symbols {
            let delta_height = required_huffman_value(self.dh_table.decode(self.stream)?)?;
            height = height
                .checked_add(delta_height)
                .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
            let width_run = self.decode_width_run(num_new_symbols, new_symbols.len())?;
            let height = i32_to_u16(height, HUFFMAN_SYMBOL_HEIGHT)?;
            let collective_bitmap = self.decode_collective_bitmap(width_run.total_width, height)?;
            append_collective_bitmap_symbols(
                &mut new_symbols,
                &collective_bitmap,
                &width_run.widths,
                height,
            )?;
        }

        Ok(new_symbols)
    }

    /// Decode one Huffman width run and return its symbol widths.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 terminates a width run with an
    /// out-of-band code from the selected `SDHUFFDW` table.
    fn decode_width_run(
        &mut self,
        num_new_symbols: usize,
        already_decoded: usize,
    ) -> Result<HuffmanWidthRun, Jbig2Error> {
        let mut symbol_width = 0i32;
        let mut total_width = 0usize;
        let mut widths = Vec::new();

        loop {
            match self.dw_table.decode(self.stream)? {
                HuffmanValue::OutOfBand => break,
                HuffmanValue::Value(delta_width) => {
                    symbol_width = symbol_width
                        .checked_add(delta_width)
                        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
                    let width = i32_to_u16(symbol_width, HUFFMAN_SYMBOL_WIDTH)?;
                    widths.push(width);
                    total_width = total_width
                        .checked_add(usize::from(width))
                        .ok_or(Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW))?;
                    let decoded_in_run = widths.len();
                    let decoded_total = already_decoded
                        .checked_add(decoded_in_run)
                        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))?;
                    if decoded_total > num_new_symbols {
                        return Err(Jbig2Error::InvalidState(SYMBOL_DICTIONARY_WIDTH_RUN));
                    }
                }
            }
        }

        Ok(HuffmanWidthRun {
            widths,
            total_width,
        })
    }

    /// Decode the collective bitmap for a Huffman width run.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 stores either an
    /// uncompressed collective bitmap when `BMSIZE` is zero or an MMR-coded
    /// collective bitmap when `BMSIZE` is positive.
    fn decode_collective_bitmap(
        &mut self,
        total_width: usize,
        height: u16,
    ) -> Result<JBig2Image, Jbig2Error> {
        let bmsize = required_huffman_value(self.bmsize_table.decode(self.stream)?)?;
        self.stream.align_to_byte_boundary();

        let total_width = usize_to_u16(total_width, COLLECTIVE_BITMAP_WIDTH)?;

        if bmsize == 0 {
            return JBig2Image::decode_uncompressed_collective_bitmap(
                total_width,
                height,
                self.stream,
            );
        }
        let mmr_len =
            i32_to_usize(bmsize).map_err(|_| Jbig2Error::InvalidState(COLLECTIVE_BITMAP_SIZE))?;
        let mmr_data = self
            .stream
            .take_from_byte_len(mmr_len)
            .ok_or(Jbig2Error::Truncated(COLLECTIVE_BITMAP))?;
        let image = decode_mmr_region(total_width, height, mmr_data)?;
        self.stream.align_to_byte_boundary();

        Ok(image)
    }

    /// Decode Huffman-coded symbol export flags.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 encodes export flags as
    /// alternating false/true runs. Huffman-coded symbol dictionaries use
    /// standard Huffman table B.1 for these run lengths.
    pub(super) fn decode_export_flags(
        &mut self,
        input_symbol_count: usize,
        new_symbol_count: usize,
    ) -> Result<Vec<bool>, Jbig2Error> {
        let total_symbols = total_symbol_count(input_symbol_count, new_symbol_count)?;
        let mut export_flags = vec![false; total_symbols];
        let mut current_flag = false;
        let mut export_index = 0usize;

        while export_index < total_symbols {
            let run_length = required_huffman_value(self.export_table.decode(self.stream)?)?;
            let run_length = i32_to_usize(run_length)?;
            export_index =
                fill_export_flag_run(&mut export_flags, export_index, run_length, current_flag)?;
            current_flag = !current_flag;
        }

        Ok(export_flags)
    }
}

/// Decoded Huffman width run metadata from section 7.4.2.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HuffmanWidthRun {
    widths: Vec<u16>,
    total_width: usize,
}

fn symbol_dictionary_table(
    selection: HuffmanTableSelection,
    custom_tables: &[CustomHuffmanDecoder],
    custom_index: &mut usize,
) -> Result<HuffmanDecoder, Jbig2Error> {
    match selection {
        HuffmanTableSelection::SymbolDictionaryDh(3)
        | HuffmanTableSelection::SymbolDictionaryDw(3) => {
            next_custom_table(custom_tables, custom_index)
        }
        _ => selection.standard_decoder().map(HuffmanDecoder::Standard),
    }
}

fn next_custom_table(
    custom_tables: &[CustomHuffmanDecoder],
    custom_index: &mut usize,
) -> Result<HuffmanDecoder, Jbig2Error> {
    let table = custom_tables
        .get(*custom_index)
        .cloned()
        .ok_or(Jbig2Error::MissingSegment)?;
    *custom_index = custom_index
        .checked_add(1)
        .ok_or(Jbig2Error::Overflow("Huffman table index overflow"))?;
    Ok(HuffmanDecoder::Custom(table))
}
