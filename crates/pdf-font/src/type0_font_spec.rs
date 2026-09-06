//! Composite Type 0 specifications and their CID descendant fonts.

use crate::{
    error::FontError,
    font::FontSource,
    pdf::{CidSystemInfo, PdfFontDescriptor, PdfMetrics, ToUnicodeMap},
    pdf_font_descriptor::{cid_to_unicode_map, descriptor},
    pdf_font_parser::{base_font, to_unicode},
    pdf_font_program::{CidCffProgram, TrueTypeProgram},
};
use pdf_cmap::{PdfCMap, Type0EncodingCMap};
use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
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
    /// Optional CID-to-glyph identifier mapping for CIDFontType2.
    pub cid_to_gid: Option<Arc<[u16]>>,
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
    pub to_unicode: Option<Arc<dyn ToUnicodeMap>>,
}

impl TryFrom<&[u8]> for CidFontKind {
    type Error = FontError;

    /// Converts the descendant dictionary's Subtype name into a supported CID kind.
    fn try_from(name: &[u8]) -> Result<Self, Self::Error> {
        match name {
            b"CIDFontType0" => Ok(Self::Type0),
            b"CIDFontType2" => Ok(Self::Type2),
            other => Err(FontError::UnsupportedCidFontSubtype {
                subtype: String::from_utf8_lossy(other).into_owned(),
            }),
        }
    }
}

impl FromPdfObject for CidFontSpec {
    /// Parses the descendant within its parent font's traversal.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let subtype: Arc<[u8]> = context.required(b"Subtype")?;
        let kind = CidFontKind::try_from(subtype.as_ref())?;
        let base_font = base_font(&mut context);
        let (descriptor, program) = match kind {
            CidFontKind::Type0 => descriptor::<CidCffProgram>(&mut context, base_font.as_deref())?,
            CidFontKind::Type2 => {
                descriptor::<TrueTypeProgram>(&mut context, base_font.as_deref())?
            }
        };
        let metrics = PdfMetrics::try_from(&mut context)?;
        let system_info = context
            .optional::<CidSystemInfo>(b"CIDSystemInfo")?
            .unwrap_or_else(|| CidSystemInfo {
                registry: Arc::default(),
                ordering: Arc::default(),
                supplement: 0,
            });
        let cid_to_unicode = cid_to_unicode_map(&system_info);
        Ok(Self {
            kind,
            descriptor,
            program,
            system_info,
            metrics,
            cid_to_gid: None,
            cid_to_unicode,
        })
    }
}

impl FromPdfObject for Type0FontSpec {
    /// Decodes the concrete font dictionary without whole-font substitution.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Self::try_from(&mut context.dictionary()?)
    }
}

impl<A: ObjectAccess + ?Sized> TryFrom<&mut DictionaryContext<'_, A>> for Type0FontSpec {
    type Error = ObjectReadError;

    /// Reads a Type 0 dictionary with exactly one descendant and an Identity-H encoding default.
    fn try_from(context: &mut DictionaryContext<'_, A>) -> ReadResult<Self> {
        let [descendant] = context.required::<[CidFontSpec; 1]>(b"DescendantFonts")?;
        let encoding = match context.optional::<Type0EncodingCMap>(b"Encoding")? {
            Some(encoding) => encoding,
            None => Type0EncodingCMap::from_name(b"Identity-H")?,
        };
        Ok(Self {
            base_font: base_font(context).unwrap_or_default(),
            encoding: Arc::new(encoding),
            descendant,
            to_unicode: to_unicode(context)?,
        })
    }
}
