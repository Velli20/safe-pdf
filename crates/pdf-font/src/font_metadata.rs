//! Style metadata derived from a PDF FontDescriptor.

use crate::{
    flags::FontFlags,
    font::{FontMetadata, FontSlant, FontWeight},
};
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

impl FromPdfObject for FontMetadata {
    /// Derives weight, slant, and symbolic style from a FontDescriptor's `/Flags`, treating
    /// malformed flags as absent. Names are filled in later from the enclosing font.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let flags = context
            .dictionary()?
            .lenient::<FontFlags>(b"Flags")
            .unwrap_or_default();
        Ok(Self {
            weight: if flags.contains(FontFlags::FORCE_BOLD) {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            },
            slant: if flags.contains(FontFlags::ITALIC) {
                FontSlant::Italic
            } else {
                FontSlant::Normal
            },
            symbolic: flags.contains(FontFlags::SYMBOLIC),
            ..Self::default()
        })
    }
}
