//! Best-effort font descriptor metadata and CID collection information.

use crate::pdf::{CidSystemInfo, PdfFontDescriptor};
use pdf_cmap::predefined::PredefinedCMap;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};
use std::{collections::HashMap, sync::Arc};

impl FromPdfObject for PdfFontDescriptor {
    /// Reads a FontDescriptor dictionary, not the enclosing font dictionary, tolerating
    /// malformed optional metadata.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let value = context.object();
        let mut context = context.dictionary()?;
        Ok(Self {
            metadata: context.read(value)?,
            bounds: context
                .dictionary()
                .optional_rect(b"FontBBox", context.source())?,
            missing_width: context.lenient(b"MissingWidth"),
            italic_angle: context.lenient(b"ItalicAngle"),
            stem_v: context.lenient(b"StemV"),
        })
    }
}

impl PdfFontDescriptor {
    /// Derives family metadata from the enclosing font's BaseFont.
    pub(crate) fn named(mut self, base_font: Option<&[u8]>) -> Self {
        let family = base_font.map(|name| Arc::<str>::from(String::from_utf8_lossy(name)));
        self.metadata.family = family.clone();
        self.metadata.postscript_name = family;
        self
    }
}

impl FromPdfObject for CidSystemInfo {
    /// Reads optional collection fields independently so malformed hints remain non-fatal.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        Ok(Self {
            registry: context.lenient(b"Registry").unwrap_or_default(),
            ordering: context.lenient(b"Ordering").unwrap_or_default(),
            supplement: context.lenient(b"Supplement").unwrap_or_default(),
        })
    }
}

impl CidSystemInfo {
    /// Finds the existing collection-based Unicode fallback when the ordering is known.
    pub(crate) fn cid_to_unicode_map(&self) -> Option<Arc<HashMap<u16, char>>> {
        let cmap_name: &[u8] = match self.ordering.as_ref() {
            b"Japan1" => b"UniJIS-UCS2-HW-H",
            b"GB1" => b"UniGB-UCS2-H",
            b"CNS1" => b"UniCNS-UCS2-H",
            b"Korea1" => b"UniKS-UCS2-H",
            _ => return None,
        };
        let cmap = PredefinedCMap::from_name(cmap_name).ok()??;
        Some(Arc::new(cmap.cid_to_unicode_map()))
    }
}
