//! Type0 font encoding CMap support.

use std::{convert::TryFrom, sync::Arc};

use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};
mod embedded;
mod parser;

use crate::{
    Cid, CidMapping, PdfCMap, PdfCode, WritingMode, cmap_support::Type0CodeMap, error::CMapError,
    predefined::PredefinedCMap,
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
}

impl PdfCMap for Type0EncodingCMap {
    /// Streams the first mapping in `bytes` using the CMap's malformed-input recovery policy.
    ///
    /// The returned mapping preserves both the packed source code and its byte length for
    /// `/ToUnicode` lookup and exact stream advancement. Identity CMaps consume two-byte codes,
    /// mapping each directly to its CID; an unmatched trailing byte is consumed as a one-byte
    /// `.notdef` mapping. Embedded and predefined maps apply their longest valid code-space match.
    fn decode_next(&self, bytes: &[u8]) -> Result<Option<CidMapping>, CMapError> {
        Ok(match self {
            Self::Identity { .. } => match bytes.get(..2).or_else(|| bytes.get(..1)) {
                Some(source) => {
                    let source = PdfCode::try_from(source)?;
                    let cid = if source.byte_len() == 2 {
                        source.value()
                    } else {
                        0
                    };
                    Some(CidMapping {
                        source,
                        cid: Cid(cid),
                    })
                }
                None => None,
            },
            Self::Predefined(cmap) => cmap.decode_next(bytes),
            Self::Embedded(cmap) => cmap.decode_next(bytes),
        })
    }

    /// Returns the writing mode declared by the identity, predefined, or embedded CMap.
    fn writing_mode(&self) -> WritingMode {
        match self {
            Self::Identity { writing_mode } => *writing_mode,
            Self::Predefined(cmap) => cmap.writing_mode(),
            Self::Embedded(cmap) => cmap.writing_mode(),
        }
    }
}

impl FromPdfObject for Type0EncodingCMap {
    /// Decodes an embedded stream or predefined name using the active object traversal.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object().value() {
            ObjectVariant::Stream(_) => {
                let context = context.stream()?;
                Ok(Self::from_bytes(context.stream().raw_data())?)
            }
            _ => {
                let name = Arc::<[u8]>::from_pdf_object(context)?;
                Ok(Self::from_name(&name)?)
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Decodes all of `text` into CIDs, one mapping at a time.
    fn decode(cmap: &Type0EncodingCMap, mut text: &[u8]) -> Vec<u32> {
        let mut decoded = Vec::new();
        while let Some(mapping) = cmap.decode_next(text).unwrap() {
            decoded.push(mapping.cid.0);
            text = &text[usize::from(mapping.source.byte_len())..];
        }
        decoded
    }

    #[test]
    fn identity_h_decodes_big_endian_pairs() {
        let cmap = Type0EncodingCMap::from_name(b"Identity-H").unwrap();

        assert_eq!(decode(&cmap, &[0x00, 0x01, 0x12, 0x34]), vec![1, 0x1234]);
        assert!(matches!(cmap, Type0EncodingCMap::Identity { .. }));
        assert_eq!(cmap.writing_mode(), WritingMode::Horizontal);
    }

    #[test]
    fn identity_h_decodes_trailing_incomplete_code_as_notdef() {
        let cmap = Type0EncodingCMap::from_name(b"Identity-H").unwrap();

        assert_eq!(decode(&cmap, &[0x00, 0x01, 0xFF]), vec![1, 0]);
    }

    #[test]
    fn identity_v_decodes_like_identity_h_with_vertical_writing_mode() {
        let cmap = Type0EncodingCMap::from_name(b"Identity-V").unwrap();

        assert_eq!(
            decode(&cmap, &[0x00, 0x01, 0x12, 0x34, 0xFF]),
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
            decode(&cmap, &[0x20, 0x01, 0x00, 0x01, 0x02]),
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

        assert_eq!(decode(&cmap, &[0x00, 0x41]), vec![7]);
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

        assert_eq!(decode(&cmap, &[0x20, 0xE3, 0x80, 0x81]), vec![1, 38]);
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

        assert_eq!(
            decode(&cmap, &[0x00, 0x01, 0x00, 0x02, 0xFF]),
            vec![4, 0, 0]
        );
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

        assert_eq!(decode(&cmap, &[0x00, 0x20, 0x00, 0x43]), vec![3, 72]);
    }

    #[test]
    fn predefined_japanese_shift_jis_cmap_decodes_single_and_double_byte_codes() {
        let cmap = Type0EncodingCMap::from_name(b"90ms-RKSJ-H").unwrap();

        assert_eq!(
            decode(&cmap, &[0x20, 0x81, 0x40, 0x7E]),
            vec![231, 633, 631]
        );
        assert_eq!(cmap.writing_mode(), WritingMode::Horizontal);
    }

    #[test]
    fn predefined_vertical_cmap_uses_horizontal_base_cmap() {
        let cmap = Type0EncodingCMap::from_name(b"90ms-RKSJ-V").unwrap();

        assert_eq!(decode(&cmap, &[0x20, 0x81, 0x40]), vec![231, 633]);
        assert_eq!(cmap.writing_mode(), WritingMode::Vertical);
    }
}
