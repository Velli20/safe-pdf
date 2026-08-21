use std::collections::HashMap;

use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{cmap::parser::CMapParser, error::CMapError};

/// A parsed ToUnicode CMap that maps PDF character codes to Unicode scalar values.
#[derive(Debug)]
pub struct ToUnicodeCMap(HashMap<u16, Vec<char>>);

impl ToUnicodeCMap {
    /// Parse the optional `/ToUnicode` CMap from a font dictionary.
    ///
    /// # Paramaters
    ///
    /// - `dictionary`: The PDF font dictionary that may contain `/ToUnicode`.
    /// - `objects`: The resolver used to dereference indirect PDF objects.
    ///
    /// # Returns
    ///
    /// The parsed ToUnicode CMap when a readable `/ToUnicode` stream is present.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, CMapError> {
        dictionary
            .get(b"ToUnicode")
            .and_then(|value| value.try_stream(objects).ok())
            .map(|stream| Self::try_from(stream.raw_data()))
            .transpose()
    }

    /// Look up the Unicode characters for the given PDF character code.
    pub fn map_char_code(&self, code: u16) -> Option<&[char]> {
        self.0.get(&code).map(Vec::as_slice)
    }
}

impl TryFrom<&[u8]> for ToUnicodeCMap {
    type Error = CMapError;

    /// Parse a ToUnicode CMap stream.
    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self(CMapParser::from(data).into_unicode_map()?))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_bfchar_entries() {
        let map = ToUnicodeCMap::try_from(
            br"
beginbfchar
<41> <0041>
<61> <0061>
endbfchar
"
            .as_slice(),
        )
        .unwrap();

        assert_eq!(map.map_char_code(0x41), Some(['A'].as_slice()));
        assert_eq!(map.map_char_code(0x61), Some(['a'].as_slice()));
        assert_eq!(map.map_char_code(0x42), None);
    }

    #[test]
    fn parses_cmap_wrapped_in_postscript_resource_boilerplate() {
        let map = ToUnicodeCMap::try_from(
            br"
/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo <</Registry (Adobe) /Ordering (Identity) /Supplement 0>> def
/CMapName /Adobe-Identity def
/CMapType 2 def
1 begincodespacerange
<0000> <FFFF>
endcodespacerange
1 beginbfchar
<0041> <0042>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end
"
            .as_slice(),
        )
        .unwrap();

        assert_eq!(map.map_char_code(0x41), Some(['B'].as_slice()));
    }

    #[test]
    fn parses_cmap_with_whitespace_after_name_solidus() {
        let map = ToUnicodeCMap::try_from(
            br"
/ CIDInit / ProcSet findresource begin
12 dict begin
begincmap
/ CIDSystemInfo
<< / Registry (Adobe)
/ Ordering (UCS) / Supplement 0 >> def
/ CMapName / Adobe-Identity-UCS def
/ CMapType 2 def
1 begincodespacerange
<00> <FF>
endcodespacerange
1 beginbfchar
<01> <D83DDCA1>
endbfchar
endcmap CMapName currentdict /CMap defineresource pop end end
"
            .as_slice(),
        )
        .unwrap();

        assert_eq!(
            map.map_char_code(1)
                .and_then(|chars| chars.first())
                .copied(),
            char::from_u32(0x1F4A1)
        );
    }

    #[test]
    fn parses_cmap_with_real_number_metadata() {
        let map = ToUnicodeCMap::try_from(
            br"
/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CMapName /Uni-Utf8-H def
/CMapVersion 1.000 def
/CMapType 2 def
1 begincodespacerange
<00> <7F>
endcodespacerange
1 beginbfchar
<41> <0042>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end
"
            .as_slice(),
        )
        .unwrap();

        assert_eq!(map.map_char_code(0x41), Some(['B'].as_slice()));
    }

    #[test]
    fn parses_bfrange_array_and_sequential_entries() {
        let map = ToUnicodeCMap::try_from(
            br"
beginbfrange
<0000> <0002> [<0041> <0042> <0043>]
<20> <22> <0045>
endbfrange
"
            .as_slice(),
        )
        .unwrap();

        assert_eq!(map.map_char_code(0), Some(['A'].as_slice()));
        assert_eq!(map.map_char_code(1), Some(['B'].as_slice()));
        assert_eq!(map.map_char_code(2), Some(['C'].as_slice()));
        assert_eq!(map.map_char_code(0x20), Some(['E'].as_slice()));
        assert_eq!(map.map_char_code(0x21), Some(['F'].as_slice()));
        assert_eq!(map.map_char_code(0x22), Some(['G'].as_slice()));
    }

    #[test]
    fn recovers_from_malformed_bfchar_destination() {
        let map = ToUnicodeCMap::try_from(
            br"
beginbfchar
<41> /bogus
endbfchar
beginbfchar
<42> <0042>
endbfchar
"
            .as_slice(),
        )
        .unwrap();

        assert_eq!(map.map_char_code(0x41), None);
        assert_eq!(map.map_char_code(0x42), Some(['B'].as_slice()));
    }

    #[test]
    fn recovers_from_malformed_bfrange_value() {
        let map = ToUnicodeCMap::try_from(
            br"
beginbfrange
<41> <42> /bogus
endbfrange
beginbfrange
<43> <44> <0043>
endbfrange
"
            .as_slice(),
        )
        .unwrap();

        assert_eq!(map.map_char_code(0x41), None);
        assert_eq!(map.map_char_code(0x43), Some(['C'].as_slice()));
        assert_eq!(map.map_char_code(0x44), Some(['D'].as_slice()));
    }

    #[test]
    fn decodes_surrogate_pairs() {
        let map = ToUnicodeCMap::try_from(b"beginbfchar\n<01> <D83DDE00>\nendbfchar\n".as_slice())
            .unwrap();

        assert_eq!(
            map.map_char_code(1)
                .and_then(|chars| chars.first())
                .copied(),
            char::from_u32(0x1F600)
        );
    }

    #[test]
    fn try_from_returns_error_for_unclosed_recovery() {
        let err = ToUnicodeCMap::try_from(b"beginbfchar\n<01> /bogus\n".as_slice()).unwrap_err();

        assert!(matches!(
            err,
            CMapError::ParserError(pdf_parser::error::ParserError::UnexpectedEndOfFile)
        ));
    }
}
