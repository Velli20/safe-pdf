//! Simple-font specifications and decoding for Type 1, MMType1, and TrueType.

use crate::{
    encoding::simple_encoding,
    error::FontError,
    font::FontSource,
    pdf::{PdfFontDescriptor, PdfMetrics, SimpleEncoding, ToUnicodeMap},
    pdf_font_descriptor::descriptor,
    pdf_font_parser::{base_font, to_unicode},
    pdf_font_program::{TrueTypeProgram, Type1Program},
    standard14::{self, Standard14Font},
};
use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
};
use std::sync::Arc;

/// Data shared by Type 1, Multiple Master Type 1, and TrueType simple fonts.
#[derive(Clone)]
pub struct SimpleFontSpec {
    /// PDF base font name without a leading slash.
    pub base_font: Arc<[u8]>,
    /// Parsed descriptor information.
    pub descriptor: PdfFontDescriptor,
    /// Embedded, external, or Standard 14 program source when available.
    pub program: Option<FontSource>,
    /// Standard 14 identity when this resource denotes one of the built-in fonts.
    pub standard14: Option<Standard14Font>,
    /// One-byte character encoding.
    pub encoding: SimpleEncoding,
    /// Explicit PDF width data.
    pub metrics: PdfMetrics,
    /// Optional source-code-to-Unicode map.
    pub to_unicode: Option<Arc<dyn ToUnicodeMap>>,
}

impl FromPdfObject for SimpleFontSpec {
    /// Decodes the concrete font dictionary without whole-font substitution.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Self::try_from(&mut context.dictionary()?)
    }
}

impl<A: ObjectAccess + ?Sized> TryFrom<&mut DictionaryContext<'_, A>> for SimpleFontSpec {
    type Error = ObjectReadError;

    /// Reads a simple font, choosing its program and encoding policy from Subtype.
    fn try_from(context: &mut DictionaryContext<'_, A>) -> ReadResult<Self> {
        let subtype: Arc<[u8]> = context.required(b"Subtype")?;
        let base_font = base_font(context);
        let (descriptor, program) = match subtype.as_ref() {
            b"Type1" | b"MMType1" => descriptor::<Type1Program>(context, base_font.as_deref())?,
            b"TrueType" => descriptor::<TrueTypeProgram>(context, base_font.as_deref())?,
            other => {
                return Err(FontError::UnsupportedFontSubtype {
                    subtype: String::from_utf8_lossy(other).into_owned(),
                }
                .into());
            }
        };
        let symbolic = subtype.as_ref() == b"TrueType" && descriptor.metadata.symbolic;
        let encoding = simple_encoding(context, symbolic)?;
        let metrics = PdfMetrics::try_from(&mut *context)?;
        let standard14 = base_font
            .as_deref()
            .and_then(standard14::from_base_font_name);
        Ok(Self {
            base_font: base_font.unwrap_or_default(),
            descriptor,
            program,
            standard14,
            encoding,
            metrics,
            to_unicode: to_unicode(context)?,
        })
    }
}
