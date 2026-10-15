//! Embedded program selection and format handling, separate from font metadata.

use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};
use std::sync::Arc;

use crate::{
    error::FontError,
    font::{FontProgramFormat, FontSource},
};

/// Type 1 program read from a FontDescriptor, with FontFile3 taking precedence over FontFile.
pub(crate) struct Type1Program(FontSource);
/// TrueType program read from a FontDescriptor's FontFile2.
pub(crate) struct TrueTypeProgram(FontSource);
/// CID-keyed CFF program read from a FontDescriptor's FontFile3.
pub(crate) struct CidType0Program(FontSource);
/// Simple-font CFF stream, either bare CFF or an OpenType container.
struct SimpleCffProgram(FontSource);
/// CID-keyed CFF stream, either bare CFF or an OpenType container.
struct CidCffProgram(FontSource);
/// Raw Type 1 stream.
struct RawType1Program(FontSource);
/// Raw TrueType stream.
struct RawTrueTypeProgram(FontSource);

impl FromPdfObject for Type1Program {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        // Presence, including an invalid value, deliberately prevents FontFile fallback.
        if context.dictionary().get(b"FontFile3").is_some() {
            context
                .required::<SimpleCffProgram>(b"FontFile3")
                .map(|program| Self(program.0))
        } else {
            context
                .required::<RawType1Program>(b"FontFile")
                .map(|program| Self(program.0))
        }
    }
}

impl FromPdfObject for TrueTypeProgram {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        context
            .dictionary()?
            .required::<RawTrueTypeProgram>(b"FontFile2")
            .map(|program| Self(program.0))
    }
}

impl FromPdfObject for CidType0Program {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        context
            .dictionary()?
            .required::<CidCffProgram>(b"FontFile3")
            .map(|program| Self(program.0))
    }
}

impl From<Type1Program> for FontSource {
    fn from(program: Type1Program) -> Self {
        program.0
    }
}

impl From<TrueTypeProgram> for FontSource {
    fn from(program: TrueTypeProgram) -> Self {
        program.0
    }
}

impl From<CidType0Program> for FontSource {
    fn from(program: CidType0Program) -> Self {
        program.0
    }
}

impl FromPdfObject for RawType1Program {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let context = context.stream()?;
        Ok(Self(FontSource::Memory {
            data: context.stream().shared_data(),
            format: FontProgramFormat::Type1,
            face_index: 0,
        }))
    }
}

impl FromPdfObject for RawTrueTypeProgram {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let context = context.stream()?;
        Ok(Self(FontSource::Memory {
            data: context.stream().shared_data(),
            format: FontProgramFormat::TrueType,
            face_index: 0,
        }))
    }
}

impl FromPdfObject for SimpleCffProgram {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.stream()?;
        let subtype = context.dictionary().optional::<Arc<[u8]>>(b"Subtype")?;
        let format = match subtype.as_deref().unwrap_or(b"OpenType") {
            b"Type1C" | b"CIDFontType0C" => FontProgramFormat::Cff,
            b"OpenType" => FontProgramFormat::OpenTypeCff,
            other => {
                return Err(FontError::UnsupportedFontSubtype {
                    subtype: String::from_utf8_lossy(other).into_owned(),
                }
                .into());
            }
        };
        Ok(Self(FontSource::Memory {
            data: context.stream().shared_data(),
            format,
            face_index: 0,
        }))
    }
}

impl FromPdfObject for CidCffProgram {
    /// Keeps standalone CID CFF raw for the Type 0 driver, which validates its structure.
    /// Type1C is accepted for producers that use that label for CID-keyed data.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.stream()?;
        let subtype = context.dictionary().optional::<Arc<[u8]>>(b"Subtype")?;
        let format = match subtype.as_deref().unwrap_or(b"OpenType") {
            b"Type1C" | b"CIDFontType0C" => FontProgramFormat::CidCff,
            b"OpenType" => FontProgramFormat::OpenTypeCff,
            other => {
                return Err(FontError::UnsupportedFontSubtype {
                    subtype: String::from_utf8_lossy(other).into_owned(),
                }
                .into());
            }
        };
        Ok(Self(FontSource::Memory {
            data: context.stream().shared_data(),
            format,
            face_index: 0,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]
mod tests {
    use super::*;
    use pdf_object_reader::{
        ObjectReader, dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    fn descriptor_with_cff(subtype: &[u8], data: &'static [u8]) -> Dictionary {
        let stream_dictionary = Dictionary::from_entries([(
            b"Subtype".as_slice(),
            pdf_object_reader::pdf_string::PdfString::from(
                subtype.to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        )]);
        let stream = StreamObject::new(1, 0, stream_dictionary, data);
        Dictionary::from_entries([(b"FontFile3".as_slice(), ObjectVariant::Stream(stream))])
    }

    fn memory_source(source: FontSource) -> (bytes::Bytes, FontProgramFormat) {
        let FontSource::Memory { data, format, .. } = source else {
            panic!("an embedded CFF program should be an in-memory source");
        };
        (data, format)
    }

    #[test]
    fn cid_font_type0c_remains_raw_cff() {
        let descriptor = descriptor_with_cff(b"CIDFontType0C", b"raw-cff");
        let source = ObjectReader::new(PassthroughResolver)
            .read::<CidType0Program>(&ObjectVariant::Dictionary(descriptor))
            .expect("the raw CID-CFF source should parse");
        let (data, format) = memory_source(FontSource::from(source));

        assert_eq!(format, FontProgramFormat::CidCff);
        assert_eq!(data.as_ref(), b"raw-cff");
    }

    #[test]
    fn cid_open_type_remains_open_type() {
        let descriptor = descriptor_with_cff(b"OpenType", b"open-type");
        let source = ObjectReader::new(PassthroughResolver)
            .read::<CidType0Program>(&ObjectVariant::Dictionary(descriptor))
            .expect("the OpenType source should parse");
        let (data, format) = memory_source(FontSource::from(source));

        assert_eq!(format, FontProgramFormat::OpenTypeCff);
        assert_eq!(data.as_ref(), b"open-type");
    }

    #[test]
    fn simple_type1c_remains_raw_cff() {
        let descriptor = descriptor_with_cff(b"Type1C", b"raw-cff");
        let source = ObjectReader::new(PassthroughResolver)
            .read::<Type1Program>(&ObjectVariant::Dictionary(descriptor))
            .expect("the raw simple CFF source should parse");
        let (data, format) = memory_source(FontSource::from(source));

        assert_eq!(format, FontProgramFormat::Cff);
        assert_eq!(data.as_ref(), b"raw-cff");
    }
}
