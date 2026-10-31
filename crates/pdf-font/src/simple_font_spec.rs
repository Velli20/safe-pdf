//! Simple-font specifications and decoding for Type 1, MMType1, and TrueType.

use crate::{
    error::FontError,
    font::FontSource,
    pdf::{PdfFontDescriptor, PdfMetrics, SimpleEncoding, ToUnicodeMap},
    pdf_font_program::{TrueTypeProgram, Type1Program},
    standard14::{self, Standard14Font},
};
use pdf_object_reader::pdf_name::PdfName;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};
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
    pub to_unicode: Option<Arc<ToUnicodeMap>>,
}

impl FromPdfObject for SimpleFontSpec {
    /// Decodes the concrete font dictionary without whole-font substitution, choosing its
    /// program and encoding policy from Subtype.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let value = context.object();
        let mut context = context.dictionary()?;
        let subtype: PdfName = context.required(b"Subtype")?;
        let base_font: Option<Arc<[u8]>> = context.lenient(b"BaseFont");
        // A bad program must not discard an otherwise usable font.
        let program: Option<FontSource> = match subtype.as_ref() {
            b"Type1" | b"MMType1" => context
                .lenient::<Type1Program>(b"FontDescriptor")
                .map(Into::into),
            b"TrueType" => context
                .lenient::<TrueTypeProgram>(b"FontDescriptor")
                .map(Into::into),
            other => {
                return Err(FontError::UnsupportedFontSubtype {
                    subtype: String::from_utf8_lossy(other).into_owned(),
                }
                .into());
            }
        };
        let descriptor = context
            .optional::<PdfFontDescriptor>(b"FontDescriptor")?
            .unwrap_or_default()
            .named(base_font.as_deref());
        let symbolic = subtype.as_ref() == b"TrueType" && descriptor.metadata.symbolic;
        let encoding = SimpleEncoding::new(context.optional(b"Encoding")?, symbolic);
        let mut metrics: PdfMetrics = context.read(value)?;
        let standard14 = base_font
            .as_deref()
            .and_then(standard14::from_base_font_name);
        if let Some(font) = standard14
            && program.is_none()
        {
            metrics.fill_standard14_widths(font, &encoding);
        }
        Ok(Self {
            base_font: base_font.unwrap_or_default(),
            descriptor,
            program,
            standard14,
            encoding,
            metrics,
            to_unicode: context.optional(b"ToUnicode")?.map(Arc::new),
        })
    }
}
