//! Composite Type 0 specifications and their CID descendant fonts.

use crate::{
    error::FontError,
    font::{FontSource, GlyphId},
    pdf::{CidSystemInfo, PdfFontDescriptor, PdfMetrics, ToUnicodeMap},
    pdf_font_program::{CidType0Program, TrueTypeProgram},
};
use pdf_cmap::{Cid, PdfCMap, Type0EncodingCMap};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};
use std::{collections::HashMap, sync::Arc};

/// Descendant subtype used by a Type 0 composite font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CidFontKind {
    /// CIDFontType0, normally backed by CID-keyed CFF outlines.
    Type0,
    /// CIDFontType2, backed by TrueType outlines.
    Type2,
}

/// Normalized descendant font used by a Type 0 composite font.
#[derive(Clone)]
pub struct CidFontSpec {
    /// Descendant subtype.
    pub kind: CidFontKind,
    /// Parsed descriptor information.
    pub descriptor: PdfFontDescriptor,
    /// Embedded or externally supplied descendant program.
    pub program: Option<FontSource>,
    /// CID collection identity.
    pub system_info: CidSystemInfo,
    /// Horizontal and vertical CID metrics.
    pub metrics: PdfMetrics,
    /// CID-to-glyph identifier mapping, used by CIDFontType2 only.
    pub cid_to_gid: CidToGidMap,
    /// Best-effort CID-to-Unicode mapping for collection-backed font substitution.
    pub cid_to_unicode: Option<Arc<HashMap<u16, char>>>,
}

/// Normalized Type 0 composite font.
#[derive(Clone)]
pub struct Type0FontSpec {
    /// PDF base font name without a leading slash.
    pub base_font: Arc<[u8]>,
    /// Source-code-to-CID encoding CMap.
    pub encoding: Arc<dyn PdfCMap>,
    /// The single descendant CID font.
    pub descendant: CidFontSpec,
    /// Optional source-code-to-Unicode map.
    pub to_unicode: Option<Arc<ToUnicodeMap>>,
}

/// `/CIDToGIDMap` of a CID descendant; CIDFontType0 descendants ignore it.
#[derive(Clone, Default)]
pub enum CidToGidMap {
    /// `/Identity` or any non-stream value: CIDs are used as glyph identifiers.
    #[default]
    Identity,
    /// Big-endian glyph identifiers indexed by CID.
    Table(Arc<[u16]>),
}

impl CidToGidMap {
    /// Returns the glyph identifier the table assigns to `cid`, or `None` for an identity map
    /// or a CID outside the table.
    pub fn glyph(&self, cid: Cid) -> Option<GlyphId> {
        let Self::Table(table) = self else {
            return None;
        };
        let gid = table.get(usize::try_from(cid.0).ok()?)?;
        Some(GlyphId(u32::from(*gid)))
    }
}

impl FromPdfObject for CidToGidMap {
    /// Decodes a stream into its `u16` table, ignoring a trailing odd byte.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let ObjectVariant::Stream(stream) = context.object().value() else {
            return Ok(Self::Identity);
        };
        let (pairs, _) = stream.raw_data().as_chunks::<2>();
        Ok(Self::Table(
            pairs.iter().copied().map(u16::from_be_bytes).collect(),
        ))
    }
}

impl FromPdfObject for CidFontKind {
    /// Decodes the descendant dictionary's Subtype name into a supported CID kind.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object().value().try_bytes(context.source())? {
            b"CIDFontType0" => Ok(Self::Type0),
            b"CIDFontType2" => Ok(Self::Type2),
            other => Err(FontError::UnsupportedCidFontSubtype {
                subtype: String::from_utf8_lossy(other).into_owned(),
            }
            .into()),
        }
    }
}

impl FromPdfObject for CidFontSpec {
    /// Parses the descendant within its parent font's traversal.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let value = context.object().value();
        let mut context = context.dictionary()?;
        let kind: CidFontKind = context.required(b"Subtype")?;
        let base_font: Option<Arc<[u8]>> = context.lenient(b"BaseFont");
        // A bad program must not discard an otherwise usable font.
        let program: Option<FontSource> = match kind {
            CidFontKind::Type0 => context
                .lenient::<CidType0Program>(b"FontDescriptor")
                .map(Into::into),
            CidFontKind::Type2 => context
                .lenient::<TrueTypeProgram>(b"FontDescriptor")
                .map(Into::into),
        };
        let descriptor = context
            .optional::<PdfFontDescriptor>(b"FontDescriptor")?
            .unwrap_or_default()
            .named(base_font.as_deref());
        let metrics: PdfMetrics = context.read(value)?;
        let system_info = context
            .optional::<CidSystemInfo>(b"CIDSystemInfo")?
            .unwrap_or_default();
        let cid_to_unicode = system_info.cid_to_unicode_map();
        let cid_to_gid = context.lenient(b"CIDToGIDMap").unwrap_or_default();
        Ok(Self {
            kind,
            descriptor,
            program,
            system_info,
            metrics,
            cid_to_gid,
            cid_to_unicode,
        })
    }
}

impl FromPdfObject for Type0FontSpec {
    /// Decodes a Type 0 dictionary without whole-font substitution, requiring exactly one
    /// descendant and defaulting to an Identity-H encoding.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let [descendant] = context.required::<[CidFontSpec; 1]>(b"DescendantFonts")?;
        let encoding = match context.optional::<Type0EncodingCMap>(b"Encoding")? {
            Some(encoding) => encoding,
            None => Type0EncodingCMap::from_name(b"Identity-H")?,
        };
        Ok(Self {
            base_font: context.lenient(b"BaseFont").unwrap_or_default(),
            encoding: Arc::new(encoding),
            descendant,
            to_unicode: context.optional(b"ToUnicode")?.map(Arc::new),
        })
    }
}
