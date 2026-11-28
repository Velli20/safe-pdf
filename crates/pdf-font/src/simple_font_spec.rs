//! Simple-font specifications and decoding for Type 1, MMType1, and TrueType.

use crate::{
    encoding::Encoding,
    error::FontError,
    font::{FontProgramFormat, FontSource},
    pdf::{PdfFontDescriptor, PdfMetrics, SimpleEncoding, ToUnicodeMap},
    pdf_font_program::{FontDescriptorEntry, TrueTypeProgram, Type1Program},
    standard14::{self, Standard14Font},
};
use pdf_object_reader::pdf_name::PdfName;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};
use std::sync::Arc;

/// The subtype of a simple font dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleFontKind {
    /// A Type 1 font.
    Type1,
    /// A Multiple Master Type 1 font.
    MultipleMasterType1,
    /// A TrueType font, including OpenType-wrapped TrueType programs.
    TrueType,
}

impl FromPdfObject for SimpleFontKind {
    /// Decodes a font dictionary's `/Subtype` name into a simple font kind.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let name = context.decode::<PdfName>()?;
        match &*name {
            b"Type1" => Ok(Self::Type1),
            b"MMType1" => Ok(Self::MultipleMasterType1),
            b"TrueType" => Ok(Self::TrueType),
            other => Err(FontError::UnsupportedFontSubtype {
                subtype: String::from_utf8_lossy(other).into_owned(),
            }
            .into()),
        }
    }
}

/// Data shared by Type 1, Multiple Master Type 1, and TrueType simple fonts.
#[derive(Clone)]
pub struct SimpleFontSpec {
    /// The font dictionary's subtype.
    pub kind: SimpleFontKind,
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
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let mut entries = context.entries()?;
        let kind: SimpleFontKind = entries.required(b"Subtype")?;
        let base_font: Option<Arc<[u8]>> = entries.lenient(b"BaseFont");
        let FontDescriptorEntry {
            descriptor,
            program,
        } = match kind {
            SimpleFontKind::Type1 | SimpleFontKind::MultipleMasterType1 => entries
                .optional::<FontDescriptorEntry<Type1Program>>(b"FontDescriptor")?
                .map(FontDescriptorEntry::into_source),
            SimpleFontKind::TrueType => entries
                .optional::<FontDescriptorEntry<TrueTypeProgram>>(b"FontDescriptor")?
                .map(FontDescriptorEntry::into_source),
        }
        .unwrap_or_default();
        // A symbolic TrueType or classic Type 1 program without `/Encoding` addresses its
        // glyphs through its own code mapping.
        let code_keyed_program = kind == SimpleFontKind::TrueType
            || matches!(
                program,
                Some(FontSource::Memory {
                    format: FontProgramFormat::Type1,
                    ..
                })
            );
        let symbolic = code_keyed_program && descriptor.symbolic;
        let standard14 = base_font
            .as_deref()
            .and_then(standard14::from_base_font_name);
        // An unknown or malformed `/Encoding` is ignored rather than discarding the font.
        // A non-embedded standard Type 1 font without `/Encoding` uses its built-in encoding,
        // which differs from the standard one for Symbol and ZapfDingbats.
        let encoding = entries.lenient::<Encoding>(b"Encoding").or_else(|| {
            standard14
                .filter(|_| kind == SimpleFontKind::Type1 && program.is_none())
                .and_then(Encoding::standard14_built_in)
        });
        // An encoding dictionary without `/BaseEncoding` differs from the built-in encoding of
        // an embedded Type 1 or CFF program, and from StandardEncoding for any other font.
        let built_in_base = matches!(
            program,
            Some(FontSource::Memory {
                format: FontProgramFormat::Type1 | FontProgramFormat::Cff,
                ..
            })
        );
        let encoding = SimpleEncoding::new(encoding, symbolic, built_in_base);
        let to_unicode = entries.lenient(b"ToUnicode").map(Arc::new);
        let mut metrics = PdfMetrics::new(descriptor.missing_width, context.decode()?);
        if let Some(font) = standard14
            && program.is_none()
        {
            metrics.fill_standard14_widths(font, &encoding);
        }
        Ok(Self {
            kind,
            base_font: base_font.unwrap_or_default(),
            descriptor,
            program,
            standard14,
            encoding,
            metrics,
            to_unicode,
        })
    }
}
