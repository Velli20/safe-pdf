//! Best-effort font descriptor metadata and CID collection information.

use crate::{
    flags::FontFlags,
    font::{FontSlant, FontSource, FontWeight},
    pdf::{CidSystemInfo, PdfFontDescriptor},
};
use pdf_cmap::predefined::PredefinedCMap;
use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ReadResult,
};
use std::{collections::HashMap, sync::Arc};

/// Metadata and the program selected for a specific font subtype.
struct Descriptor<P> {
    metadata: PdfFontDescriptor,
    program: Option<P>,
}

impl<P: FromPdfObject + Into<FontSource>> FromPdfObject for Descriptor<P> {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let value = context.object().value();
        let mut context = context.dictionary()?;
        // Bad optional hints or programs must not discard an otherwise usable font.
        let metadata: PdfFontDescriptor = context.read(value)?;
        let program = context.read::<P>(value).ok();
        Ok(Self { metadata, program })
    }
}

impl FromPdfObject for PdfFontDescriptor {
    /// Reads a FontDescriptor dictionary, not the enclosing font dictionary, tolerating
    /// malformed optional metadata.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let flags = context
            .optional::<FontFlags>(b"Flags")
            .ok()
            .flatten()
            .unwrap_or_default();
        let bounds = context
            .dictionary()
            .optional_rect(b"FontBBox", context.source())
            .ok()
            .flatten();
        let mut descriptor = Self {
            bounds,
            missing_width: context.optional(b"MissingWidth").ok().flatten(),
            italic_angle: context.optional(b"ItalicAngle").ok().flatten(),
            stem_v: context.optional(b"StemV").ok().flatten(),
            ..Self::default()
        };
        descriptor.metadata.weight = FontWeight(if flags.contains(FontFlags::FORCE_BOLD) {
            700
        } else {
            400
        });
        descriptor.metadata.slant = if flags.contains(FontFlags::ITALIC) {
            FontSlant::Italic
        } else {
            FontSlant::Normal
        };
        descriptor.metadata.symbolic = flags.contains(FontFlags::SYMBOLIC);
        Ok(descriptor)
    }
}

/// Reads the descriptor once and derives family metadata from the enclosing BaseFont.
pub(crate) fn descriptor<P: FromPdfObject + Into<FontSource>>(
    context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>,
    base_font: Option<&[u8]>,
) -> ReadResult<(PdfFontDescriptor, Option<FontSource>)> {
    let (mut descriptor, program) = match context.optional::<Descriptor<P>>(b"FontDescriptor")? {
        Some(value) => (value.metadata, value.program.map(Into::into)),
        None => (PdfFontDescriptor::default(), None),
    };
    let family = base_font.map(|name| Arc::<str>::from(String::from_utf8_lossy(name).into_owned()));
    descriptor.metadata.family = family.clone();
    descriptor.metadata.postscript_name = family;
    Ok((descriptor, program))
}

impl FromPdfObject for CidSystemInfo {
    /// Reads optional collection fields independently so malformed hints remain non-fatal.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        Ok(Self {
            registry: context
                .optional(b"Registry")
                .ok()
                .flatten()
                .unwrap_or_default(),
            ordering: context
                .optional(b"Ordering")
                .ok()
                .flatten()
                .unwrap_or_default(),
            supplement: context
                .optional(b"Supplement")
                .ok()
                .flatten()
                .unwrap_or_default(),
        })
    }
}

/// Finds the existing collection-based Unicode fallback when the ordering is known.
pub(crate) fn cid_to_unicode_map(system_info: &CidSystemInfo) -> Option<Arc<HashMap<u16, char>>> {
    let cmap_name: &[u8] = match system_info.ordering.as_ref() {
        b"Japan1" => b"UniJIS-UCS2-HW-H",
        b"GB1" => b"UniGB-UCS2-H",
        b"CNS1" => b"UniCNS-UCS2-H",
        b"Korea1" => b"UniKS-UCS2-H",
        _ => return None,
    };
    PredefinedCMap::from_name(cmap_name)
        .ok()
        .flatten()
        .map(|cmap| Arc::new(cmap.cid_to_unicode_map()))
}
