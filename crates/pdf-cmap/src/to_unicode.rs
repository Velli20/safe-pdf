//! Parsing and compact storage for PDF `/ToUnicode` character maps.
//!
//! Parsed vectors are converted once into [`UnicodeSequence`], keeping the common zero- and
//! one-scalar mappings inline and sharing storage only for genuine multi-scalar expansions.

use std::collections::HashMap;

use crate::{UnicodeSequence, cmap::parser::CMapParser, error::CMapError};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, dictionary::Dictionary,
    object_resolver::ObjectResolver,
};

/// A named identity `/ToUnicode` map covering the Unicode basic multilingual plane.
///
/// Some producers write `/ToUnicode /Identity-H` or `/Identity-V` instead of
/// embedding the stream required by the PDF specification. Common readers
/// interpret those names as a direct character-code-to-Unicode mapping.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityToUnicodeMap;

impl IdentityToUnicodeMap {
    /// Returns an identity map for the supported horizontal and vertical names.
    #[must_use]
    pub fn from_name(name: &[u8]) -> Option<Self> {
        matches!(name, b"Identity-H" | b"Identity-V").then_some(Self)
    }
}

impl crate::ToUnicodeMap for IdentityToUnicodeMap {
    fn map(&self, code: crate::PdfCode) -> Option<UnicodeSequence> {
        u16::try_from(code.value())
            .ok()
            .and_then(|value| char::from_u32(u32::from(value)))
            .map(Into::into)
    }
}

/// A parsed ToUnicode CMap that maps PDF character codes to Unicode scalar values.
#[derive(Debug)]
pub struct ToUnicodeCMap(HashMap<u16, UnicodeSequence>);

impl ToUnicodeCMap {
    /// Parse the optional `/ToUnicode` CMap from a font dictionary.
    ///
    /// # Parameters
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

    /// Looks up the Unicode characters for the given PDF character code.
    ///
    /// The returned sequence is borrowed from the map; callers that need to retain it can clone the
    /// compact value without cloning a multi-scalar buffer.
    pub fn map_char_code(&self, code: u16) -> Option<&UnicodeSequence> {
        self.0.get(&code)
    }
}

impl TryFrom<&[u8]> for ToUnicodeCMap {
    type Error = CMapError;

    /// Parses a `/ToUnicode` CMap stream and compacts each parsed destination sequence.
    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let mappings = CMapParser::from(data)
            .into_unicode_map()?
            .into_iter()
            .map(|(code, characters)| (code, UnicodeSequence::from(characters)))
            .collect();
        Ok(Self(mappings))
    }
}

impl crate::ToUnicodeMap for ToUnicodeCMap {
    fn map(&self, code: crate::PdfCode) -> Option<UnicodeSequence> {
        u16::try_from(code.value())
            .ok()
            .and_then(|value| self.map_char_code(value).cloned())
    }
}

impl FromPdfObject for ToUnicodeCMap {
    /// Parses a stream within the caller's traversal; named identity maps are handled separately.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let context = context.stream()?;
        Ok(Self::try_from(context.stream().raw_data())?)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{PdfCode, ToUnicodeMap};

    fn mapped(map: &ToUnicodeCMap, code: u16) -> Option<&[char]> {
        map.map_char_code(code).map(UnicodeSequence::as_slice)
    }

    #[test]
    fn identity_map_maps_valid_bmp_codes() {
        let map = IdentityToUnicodeMap::from_name(b"Identity-H").unwrap();

        for (code, expected) in [(0x55, 'U'), (0x11B, 'ě'), (0xED, 'í'), (0x2013, '–')] {
            let code = PdfCode::new(code, 2).unwrap();
            assert_eq!(map.map(code), Some(expected.into()));
        }
        assert!(IdentityToUnicodeMap::from_name(b"Identity-V").is_some());
        assert!(IdentityToUnicodeMap::from_name(b"Adobe-Identity-UCS").is_none());
    }

    #[test]
    fn identity_map_rejects_surrogates_and_codes_outside_the_bmp() {
        let map = IdentityToUnicodeMap;

        assert_eq!(map.map(PdfCode::new(0xD800, 2).unwrap()), None);
        assert_eq!(map.map(PdfCode::new(0x10000, 3).unwrap()), None);
    }

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

        assert_eq!(
            map.map_char_code(0x41).map(UnicodeSequence::as_slice),
            Some(['A'].as_slice())
        );
        assert_eq!(
            map.map_char_code(0x61).map(UnicodeSequence::as_slice),
            Some(['a'].as_slice())
        );
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

        assert_eq!(
            map.map_char_code(0x41).map(UnicodeSequence::as_slice),
            Some(['B'].as_slice())
        );
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
                .and_then(|chars| chars.as_slice().first())
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

        assert_eq!(
            map.map_char_code(0x41).map(UnicodeSequence::as_slice),
            Some(['B'].as_slice())
        );
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

        assert_eq!(mapped(&map, 0), Some(['A'].as_slice()));
        assert_eq!(mapped(&map, 1), Some(['B'].as_slice()));
        assert_eq!(mapped(&map, 2), Some(['C'].as_slice()));
        assert_eq!(mapped(&map, 0x20), Some(['E'].as_slice()));
        assert_eq!(mapped(&map, 0x21), Some(['F'].as_slice()));
        assert_eq!(mapped(&map, 0x22), Some(['G'].as_slice()));
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
        assert_eq!(mapped(&map, 0x42), Some(['B'].as_slice()));
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
        assert_eq!(mapped(&map, 0x43), Some(['C'].as_slice()));
        assert_eq!(mapped(&map, 0x44), Some(['D'].as_slice()));
    }

    #[test]
    fn decodes_surrogate_pairs() {
        let map = ToUnicodeCMap::try_from(b"beginbfchar\n<01> <D83DDE00>\nendbfchar\n".as_slice())
            .unwrap();

        assert_eq!(
            map.map_char_code(1)
                .and_then(|chars| chars.as_slice().first())
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
