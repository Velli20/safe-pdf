//! Type0 font encoding CMap support.

use std::convert::TryFrom;

use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
    text_encoding::BigEndianU16Units,
};

mod embedded;
mod parser;

use crate::{
    WritingMode, cmap_support::Type0CodeMap, error::CMapError, predefined::PredefinedCMap,
};

pub use embedded::EmbeddedCMap;

/// Parsed representation of a Type0 font `/Encoding` CMap.
#[derive(Debug, Clone)]
pub enum Type0EncodingCMap {
    /// Predefined identity mapping with an associated writing mode.
    Identity { writing_mode: WritingMode },
    /// Named predefined CMap generated at build time.
    Predefined(PredefinedCMap),
    /// Embedded CMap data parsed from a stream object.
    Embedded(EmbeddedCMap),
}

impl Type0EncodingCMap {
    /// Parse the optional `/Encoding` entry of a Type0 font dictionary.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Self>, CMapError> {
        dictionary
            .get(b"Encoding")
            .map(|value| {
                let resolved = objects.resolve_object(value)?;
                match resolved {
                    ObjectVariant::Stream(stream) => Self::from_bytes(stream.raw_data()),
                    _ => Self::from_name(value.try_bytes(objects)?),
                }
            })
            .transpose()
    }

    /// Build a Type0 encoding CMap from a predefined CMap name.
    pub fn from_name(name: &[u8]) -> Result<Self, CMapError> {
        if let Ok(writing_mode) = WritingMode::try_from(name) {
            return Ok(Self::Identity { writing_mode });
        }

        PredefinedCMap::from_name(name)?
            .map(Self::Predefined)
            .ok_or_else(|| {
                CMapError::UnsupportedType0EncodingCMap(String::from_utf8_lossy(name).into_owned())
            })
    }

    /// Parse an embedded Type0 encoding CMap stream.
    ///
    /// This delegates to `EmbeddedCMap::try_from`, which parses raw stream
    /// bytes and returns `CMapError` for malformed embedded CMap data.
    pub fn from_bytes(data: &[u8]) -> Result<Self, CMapError> {
        EmbeddedCMap::try_from(data).map(Self::Embedded)
    }

    /// Decode raw text bytes using the predefined Identity-H/V mapping.
    ///
    /// Identity CMaps define fixed-width 2-byte big-endian character codes,
    /// and each code maps directly to the resulting CID.
    pub fn decode_identity(text: &[u8]) -> Vec<u16> {
        let decoded = BigEndianU16Units::from(text);
        let mut units = decoded.units;

        // A non-empty remainder means the source text ended with an incomplete
        // final code. That is malformed input, but we keep the decode
        // infallible and emit CID 0 (`.notdef`) as the best-effort replacement.
        if decoded.trailing_byte.is_some() {
            units.push(0);
        }

        units
    }

    /// Decode raw text bytes into CIDs using this CMap.
    pub fn decode(&self, text: &[u8]) -> Vec<u16> {
        match self {
            Self::Identity { .. } => Self::decode_identity(text),
            Self::Predefined(cmap) => cmap.decode_text(text),
            Self::Embedded(cmap) => cmap.decode_text(text),
        }
    }

    /// Return whether this CMap is one of the predefined identity mappings.
    pub fn is_identity(&self) -> bool {
        matches!(self, Self::Identity { .. })
    }

    /// Return the writing mode declared by this CMap.
    pub fn writing_mode(&self) -> WritingMode {
        match self {
            Self::Identity { writing_mode } => *writing_mode,
            Self::Predefined(cmap) => cmap.writing_mode(),
            Self::Embedded(cmap) => cmap.writing_mode(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn identity_h_decodes_big_endian_pairs() {
        let cmap = Type0EncodingCMap::from_name(b"Identity-H").unwrap();

        assert_eq!(cmap.decode(&[0x00, 0x01, 0x12, 0x34]), vec![1, 0x1234]);
        assert!(cmap.is_identity());
        assert_eq!(cmap.writing_mode(), WritingMode::Horizontal);
    }

    #[test]
    fn identity_h_decodes_trailing_incomplete_code_as_notdef() {
        let cmap = Type0EncodingCMap::from_name(b"Identity-H").unwrap();

        assert_eq!(cmap.decode(&[0x00, 0x01, 0xFF]), vec![1, 0]);
    }

    #[test]
    fn decode_identity_handles_complete_pairs_empty_input_and_trailing_byte() {
        assert_eq!(Type0EncodingCMap::decode_identity(&[0xFF]), vec![0]);
        assert_eq!(
            Type0EncodingCMap::decode_identity(&[0x00, 0x01, 0x12, 0x34]),
            vec![1, 0x1234]
        );
        assert_eq!(Type0EncodingCMap::decode_identity(&[]), Vec::<u16>::new());
    }

    #[test]
    fn identity_v_decodes_like_identity_h_with_vertical_writing_mode() {
        let cmap = Type0EncodingCMap::from_name(b"Identity-V").unwrap();

        assert_eq!(
            cmap.decode(&[0x00, 0x01, 0x12, 0x34, 0xFF]),
            vec![1, 0x1234, 0]
        );
        assert_eq!(cmap.writing_mode(), WritingMode::Vertical);
    }

    #[test]
    fn embedded_cmap_decodes_cidchar_and_cidrange_entries() {
        let data = br#"
        begincmap
        /WMode 0 def
        2 begincodespacerange
        <00> <FF>
        <0100> <01FF>
        endcodespacerange
        1 begincidchar
        <20> 7
        endcidchar
        1 begincidrange
        <0100> <0102> 50
        endcidrange
        endcmap
        "#;

        let cmap = Type0EncodingCMap::from_bytes(data).unwrap();

        assert_eq!(
            cmap.decode(&[0x20, 0x01, 0x00, 0x01, 0x02]),
            vec![7, 50, 52]
        );
        assert_eq!(cmap.writing_mode(), WritingMode::Horizontal);
    }

    #[test]
    fn embedded_cmap_accepts_postscript_resource_boilerplate() {
        let data = br#"
        /CIDInit /ProcSet findresource begin
        12 dict begin
        begincmap
        /WMode 0 def
        1 begincodespacerange
        <0000> <00FF>
        endcodespacerange
        1 begincidchar
        <0041> 7
        endcidchar
        endcmap
        CMapName currentdict /CMap defineresource pop
        end
        end
        "#;

        let cmap = Type0EncodingCMap::from_bytes(data).unwrap();

        assert_eq!(cmap.decode(&[0x00, 0x41]), vec![7]);
        assert_eq!(cmap.writing_mode(), WritingMode::Horizontal);
    }

    #[test]
    fn embedded_cmap_accepts_real_number_metadata() {
        let data = br#"
        /CIDInit /ProcSet findresource begin
        12 dict begin
        begincmap
        /CMapName /Uni-Utf8-H def
        /CMapVersion 1.000 def
        /CMapType 1 def
        /WMode 0 def
        2 begincodespacerange
        <00> <7F>
        <E08080> <EFBFBF>
        endcodespacerange
        2 begincidchar
        <20> 1
        <e38081> 38
        endcidchar
        endcmap
        CMapName currentdict /CMap defineresource pop
        end
        end
        "#;

        let cmap = Type0EncodingCMap::from_bytes(data).unwrap();

        assert_eq!(cmap.decode(&[0x20, 0xE3, 0x80, 0x81]), vec![1, 38]);
    }

    #[test]
    fn embedded_cmap_uses_notdef_for_unmapped_or_invalid_codes() {
        let data = br#"
        begincmap
        /WMode 1 def
        1 begincodespacerange
        <0001> <00FF>
        endcodespacerange
        1 begincidchar
        <0001> 4
        endcidchar
        endcmap
        "#;

        let cmap = Type0EncodingCMap::from_bytes(data).unwrap();

        assert_eq!(cmap.decode(&[0x00, 0x01, 0x00, 0x02, 0xFF]), vec![4, 0, 0]);
        assert_eq!(cmap.writing_mode(), WritingMode::Vertical);
    }

    #[test]
    fn embedded_cmap_decodes_bf_entries_as_cids() {
        let data = br#"
        begincmap
        1 begincodespacerange
        <0020> <0043>
        endcodespacerange
        1 beginbfchar
        <0020> <0003>
        endbfchar
        1 beginbfrange
        <0041> <0043> <0046>
        endbfrange
        endcmap
        "#;

        let cmap = Type0EncodingCMap::from_bytes(data).unwrap();

        assert_eq!(cmap.decode(&[0x00, 0x20, 0x00, 0x43]), vec![3, 72]);
    }

    #[test]
    fn predefined_japanese_shift_jis_cmap_decodes_single_and_double_byte_codes() {
        let cmap = Type0EncodingCMap::from_name(b"90ms-RKSJ-H").unwrap();

        assert_eq!(cmap.decode(&[0x20, 0x81, 0x40, 0x7E]), vec![231, 633, 631]);
        assert_eq!(cmap.writing_mode(), WritingMode::Horizontal);
    }

    #[test]
    fn predefined_vertical_cmap_uses_horizontal_base_cmap() {
        let cmap = Type0EncodingCMap::from_name(b"90ms-RKSJ-V").unwrap();

        assert_eq!(cmap.decode(&[0x20, 0x81, 0x40]), vec![231, 633]);
        assert_eq!(cmap.writing_mode(), WritingMode::Vertical);
    }
}
