use crate::error::Jbig2Error;

use super::{CustomHuffmanDecoder, HuffmanDecoder, HuffmanTableSelection};

/// Stateful cursor over referred custom Huffman tables.
#[derive(Debug, Clone)]
pub(crate) struct CustomHuffmanTableCursor {
    tables: Vec<CustomHuffmanDecoder>,
}

impl CustomHuffmanTableCursor {
    /// Create a cursor that will yield referred custom tables in declaration order.
    ///
    /// The tables are reversed once up front so `pop()` can consume them without
    /// cloning while preserving the original front-to-back table order.
    pub(crate) fn new(mut tables: Vec<CustomHuffmanDecoder>) -> Self {
        tables.reverse();
        Self { tables }
    }

    /// Resolve a symbol-dictionary selector to either a standard or custom table.
    ///
    /// Selector `3` consumes the next referred custom table; all other selectors
    /// use the built-in Annex B standard table for the requested role.
    pub(crate) fn symbol_dictionary_table(
        &mut self,
        selection: HuffmanTableSelection,
    ) -> Result<HuffmanDecoder, Jbig2Error> {
        match selection {
            HuffmanTableSelection::SymbolDictionaryDh(3)
            | HuffmanTableSelection::SymbolDictionaryDw(3) => self.next_decoder(),
            _ => selection.standard_decoder().map(HuffmanDecoder::Standard),
        }
    }

    /// Resolve a text-region selector to either a standard or custom table.
    ///
    /// Selector `3` consumes the next referred custom table; all other selectors
    /// use the built-in Annex B standard table for the requested role.
    pub(crate) fn text_region_table(
        &mut self,
        selection: HuffmanTableSelection,
    ) -> Result<HuffmanDecoder, Jbig2Error> {
        match selection {
            HuffmanTableSelection::TextRegionFs(3)
            | HuffmanTableSelection::TextRegionDs(3)
            | HuffmanTableSelection::TextRegionDt(3) => self.next_decoder(),
            _ => selection.standard_decoder().map(HuffmanDecoder::Standard),
        }
    }

    /// Resolve a text-region refinement selector to either a standard or custom table.
    ///
    /// Selectors `0` and `1` use the built-in Annex B tables `B.14` and `B.15`.
    /// Selector `3` consumes the next referred custom table.
    pub(crate) fn text_region_refinement_table(
        &mut self,
        selector: u8,
    ) -> Result<HuffmanDecoder, Jbig2Error> {
        match selector {
            3 => self.next_decoder(),
            _ => super::text_region_refinement_standard_decoder(selector)
                .map(HuffmanDecoder::Standard),
        }
    }

    /// Resolve the text-region refinement-size table.
    ///
    /// `SBHUFFRSIZE = 0` uses the built-in Annex B table `B.1`; otherwise the
    /// next referred custom table is consumed.
    pub(crate) fn text_region_rsize_table(
        &mut self,
        custom: bool,
    ) -> Result<HuffmanDecoder, Jbig2Error> {
        if custom {
            return self.next_decoder();
        }

        Ok(HuffmanDecoder::Standard(
            super::StandardHuffmanDecoder::new(super::STANDARD_TABLE_B1)?,
        ))
    }

    /// Consume and return the next referred custom Huffman table.
    ///
    /// Returns `MissingSegment` when no more referred tables are available.
    pub(crate) fn next_decoder(&mut self) -> Result<HuffmanDecoder, Jbig2Error> {
        let table = self.tables.pop().ok_or(Jbig2Error::MissingSegment)?;
        Ok(HuffmanDecoder::Custom(table))
    }
}

#[cfg(test)]
mod tests {
    use super::CustomHuffmanTableCursor;
    use crate::{
        error::Jbig2Error,
        huffman::{
            CustomHuffmanDecoder, HuffmanDecoder, HuffmanTableSelection, StandardHuffmanDecoder,
            standard::{STANDARD_TABLE_B1, STANDARD_TABLE_B6, STANDARD_TABLE_B14},
            test_support::{bits_to_bytes, push_bits},
        },
    };

    fn custom_table(seed: i32) -> CustomHuffmanDecoder {
        let mut data = Vec::new();
        data.push(0);
        data.extend_from_slice(&seed.to_be_bytes());
        data.extend_from_slice(&seed.to_be_bytes());
        let bits = [true, true];
        data.extend_from_slice(&bits_to_bytes(&bits));
        CustomHuffmanDecoder::parse(&data).expect("custom table")
    }

    #[test]
    fn consumes_custom_tables_in_sequence() {
        let tables = vec![custom_table(1), custom_table(2)];
        let mut cursor = CustomHuffmanTableCursor::new(tables.clone());

        assert_eq!(
            cursor.next_decoder(),
            Ok(HuffmanDecoder::Custom(tables[0].clone()))
        );
        assert_eq!(
            cursor.next_decoder(),
            Ok(HuffmanDecoder::Custom(tables[1].clone()))
        );
    }

    #[test]
    fn selects_standard_symbol_dictionary_table_without_consuming_custom() {
        let tables = vec![custom_table(1)];
        let mut cursor = CustomHuffmanTableCursor::new(tables.clone());

        assert_eq!(
            cursor.symbol_dictionary_table(HuffmanTableSelection::SymbolDictionaryDh(0)),
            Ok(HuffmanDecoder::Standard(
                StandardHuffmanDecoder::new(crate::huffman::STANDARD_TABLE_B4)
                    .expect("standard table")
            ))
        );
        assert_eq!(
            cursor.next_decoder(),
            Ok(HuffmanDecoder::Custom(tables[0].clone()))
        );
    }

    #[test]
    fn selects_custom_symbol_dictionary_table_for_selector_three() {
        let tables = vec![custom_table(7)];
        let mut cursor = CustomHuffmanTableCursor::new(tables.clone());

        assert_eq!(
            cursor.symbol_dictionary_table(HuffmanTableSelection::SymbolDictionaryDw(3)),
            Ok(HuffmanDecoder::Custom(tables[0].clone()))
        );
    }

    #[test]
    fn selects_custom_text_region_table_for_selector_three() {
        let tables = vec![custom_table(9)];
        let mut cursor = CustomHuffmanTableCursor::new(tables.clone());

        assert_eq!(
            cursor.text_region_table(HuffmanTableSelection::TextRegionDt(3)),
            Ok(HuffmanDecoder::Custom(tables[0].clone()))
        );
    }

    #[test]
    fn selects_standard_text_region_table_without_consuming_custom() {
        let tables = vec![custom_table(1)];
        let mut cursor = CustomHuffmanTableCursor::new(tables.clone());

        assert_eq!(
            cursor.text_region_table(HuffmanTableSelection::TextRegionFs(0)),
            Ok(HuffmanDecoder::Standard(
                StandardHuffmanDecoder::new(STANDARD_TABLE_B6).expect("standard table")
            ))
        );
        assert_eq!(
            cursor.next_decoder(),
            Ok(HuffmanDecoder::Custom(tables[0].clone()))
        );
    }

    #[test]
    fn selects_custom_text_region_refinement_table_for_selector_three() {
        let tables = vec![custom_table(9)];
        let mut cursor = CustomHuffmanTableCursor::new(tables.clone());

        assert_eq!(
            cursor.text_region_refinement_table(3),
            Ok(HuffmanDecoder::Custom(tables[0].clone()))
        );
    }

    #[test]
    fn selects_standard_text_region_refinement_table_without_consuming_custom() {
        let tables = vec![custom_table(1)];
        let mut cursor = CustomHuffmanTableCursor::new(tables.clone());

        assert_eq!(
            cursor.text_region_refinement_table(0),
            Ok(HuffmanDecoder::Standard(
                StandardHuffmanDecoder::new(STANDARD_TABLE_B14).expect("standard table")
            ))
        );
        assert_eq!(
            cursor.next_decoder(),
            Ok(HuffmanDecoder::Custom(tables[0].clone()))
        );
    }

    #[test]
    fn selects_custom_rsize_table() {
        let tables = vec![custom_table(11)];
        let mut cursor = CustomHuffmanTableCursor::new(tables.clone());

        assert_eq!(
            cursor.text_region_rsize_table(true),
            Ok(HuffmanDecoder::Custom(tables[0].clone()))
        );
    }

    #[test]
    fn selects_standard_rsize_table_without_consuming_custom() {
        let tables = vec![custom_table(1)];
        let mut cursor = CustomHuffmanTableCursor::new(tables.clone());

        assert_eq!(
            cursor.text_region_rsize_table(false),
            Ok(HuffmanDecoder::Standard(
                StandardHuffmanDecoder::new(STANDARD_TABLE_B1).expect("standard table")
            ))
        );
        assert_eq!(
            cursor.next_decoder(),
            Ok(HuffmanDecoder::Custom(tables[0].clone()))
        );
    }

    #[test]
    fn consumes_mixed_text_region_custom_tables_in_header_order() {
        let tables = vec![
            custom_table(1),
            custom_table(2),
            custom_table(3),
            custom_table(4),
        ];
        let mut cursor = CustomHuffmanTableCursor::new(tables.clone());

        assert_eq!(
            cursor.text_region_table(HuffmanTableSelection::TextRegionFs(3)),
            Ok(HuffmanDecoder::Custom(tables[0].clone()))
        );
        assert_eq!(
            cursor.text_region_refinement_table(3),
            Ok(HuffmanDecoder::Custom(tables[1].clone()))
        );
        assert_eq!(
            cursor.text_region_refinement_table(3),
            Ok(HuffmanDecoder::Custom(tables[2].clone()))
        );
        assert_eq!(
            cursor.text_region_rsize_table(true),
            Ok(HuffmanDecoder::Custom(tables[3].clone()))
        );
    }

    #[test]
    fn returns_missing_segment_when_custom_table_is_absent() {
        let mut cursor = CustomHuffmanTableCursor::new(Vec::new());

        assert_eq!(cursor.next_decoder(), Err(Jbig2Error::MissingSegment));
    }

    #[test]
    fn custom_table_fixture_parses() {
        let mut bits = Vec::new();
        push_bits(&mut bits, 0, 1);
        let data = bits_to_bytes(&bits);
        assert!(!data.is_empty());
    }
}
