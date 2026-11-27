//! Best-effort font descriptor metadata and CID collection information.

use crate::{
    font::LineMetrics,
    pdf::{CidSystemInfo, PDF_GLYPH_SPACE_UNITS_PER_EM, PdfFontDescriptor},
    type0_font_spec::CidToUnicode,
};
use pdf_cmap::predefined::PredefinedCMap;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};
use std::sync::Arc;

impl FromPdfObject for PdfFontDescriptor {
    /// Reads a FontDescriptor dictionary, not the enclosing font dictionary, tolerating
    /// malformed optional metadata.
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let mut entries = context.entries()?;
        let bounds = entries.lenient(b"FontBBox");
        let missing_width = entries.lenient(b"MissingWidth");
        let italic_angle = entries.lenient(b"ItalicAngle");
        let stem_v = entries.lenient(b"StemV");
        let ascent = entries.lenient(b"Ascent");
        let descent = entries.lenient(b"Descent");
        Ok(Self {
            metadata: context.decode()?,
            bounds,
            missing_width,
            italic_angle,
            stem_v,
            ascent,
            descent,
        })
    }
}

/// A declared `/Descent` above this many thousandths of an em is a depth written without
/// its sign; smaller positive values are too close to the baseline to tell apart and are
/// kept as they are.
const MISSIGNED_DESCENT: f32 = 10.0;

impl PdfFontDescriptor {
    /// The declared `/Ascent` and `/Descent` as fractions of an em, as viewers lay out
    /// form text: a positive descent is read as its negation, and a descriptor declaring
    /// neither, or both as zero, gives `None`.
    pub(crate) fn line_metrics(&self) -> Option<LineMetrics> {
        let ascent = self.ascent.unwrap_or_default();
        let descent = self.descent.unwrap_or_default();
        let descent = if descent > MISSIGNED_DESCENT {
            -descent
        } else {
            descent
        };
        (ascent != 0.0 || descent != 0.0).then(|| LineMetrics {
            ascent: ascent / PDF_GLYPH_SPACE_UNITS_PER_EM,
            descent: descent / PDF_GLYPH_SPACE_UNITS_PER_EM,
        })
    }

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
    /// Finds the collection-based Unicode fallback when the ordering is known.
    pub(crate) fn cid_to_unicode_map(&self) -> Option<CidToUnicode> {
        let cmap_name: &[u8] = match self.ordering.as_ref() {
            b"Japan1" => b"UniJIS-UCS2-HW-H",
            b"GB1" => b"UniGB-UCS2-H",
            b"CNS1" => b"UniCNS-UCS2-H",
            b"Korea1" => b"UniKS-UCS2-H",
            _ => return None,
        };
        let cmap = PredefinedCMap::from_name(cmap_name).ok()??;
        Some(CidToUnicode::Collection(Arc::new(
            cmap.cid_to_unicode_map(),
        )))
    }
}
