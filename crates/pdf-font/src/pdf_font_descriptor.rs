//! Best-effort font descriptor metadata and CID collection information.

use crate::{
    flags::FontFlags,
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
    ///
    /// The declared `/Ascent` and `/Descent` become a line extent as viewers lay out form
    /// text: a positive descent is read as its negation.
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let mut entries = context.entries()?;
        let missing_width = entries.lenient(b"MissingWidth");
        let ascent: f32 = entries.lenient(b"Ascent").unwrap_or_default();
        let descent: f32 = entries.lenient(b"Descent").unwrap_or_default();
        let descent = if descent > MISSIGNED_DESCENT {
            -descent
        } else {
            descent
        };
        let line = (ascent != 0.0 || descent != 0.0).then(|| LineMetrics {
            ascent: ascent / PDF_GLYPH_SPACE_UNITS_PER_EM,
            descent: descent / PDF_GLYPH_SPACE_UNITS_PER_EM,
        });
        // Malformed flags are treated as absent.
        let symbolic = entries
            .lenient::<FontFlags>(b"Flags")
            .is_some_and(|flags| flags.contains(FontFlags::SYMBOLIC));
        Ok(Self {
            symbolic,
            missing_width,
            line,
        })
    }
}

/// A declared `/Descent` above this many thousandths of an em is a depth written without
/// its sign; smaller positive values are too close to the baseline to tell apart and are
/// kept as they are.
const MISSIGNED_DESCENT: f32 = 10.0;

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
