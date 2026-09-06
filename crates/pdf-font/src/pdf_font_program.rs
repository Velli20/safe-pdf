//! Embedded program selection and format handling, separate from font metadata.

use bytes::Bytes;
use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ReadResult,
};
use std::sync::Arc;

use crate::{
    cff_builder::build_cff_font,
    error::FontError,
    font::{FontProgramFormat, FontSource},
};

/// Selects the descriptor entry appropriate to a font subtype.
pub(crate) trait DescriptorProgram: Sized {
    /// Reads the embedded program from an already active descriptor context.
    fn read(context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self>;
    /// Returns the normalized in-memory source.
    fn into_source(self) -> FontSource;
}

/// Type 1 program, with FontFile3 taking precedence over FontFile.
pub(crate) struct Type1Program(FontSource);
/// TrueType program stored in FontFile2.
pub(crate) struct TrueTypeProgram(FontSource);
/// CID-keyed CFF program stored in FontFile3.
pub(crate) struct CidCffProgram(FontSource);
/// Simple CFF stream whose standalone data needs an OpenType wrapper.
struct SimpleCffProgram(FontSource);
/// Raw Type 1 stream.
struct RawType1Program(FontSource);

impl DescriptorProgram for Type1Program {
    fn read(context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
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
    fn into_source(self) -> FontSource {
        self.0
    }
}

impl DescriptorProgram for TrueTypeProgram {
    fn read(context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        context.required(b"FontFile2")
    }
    fn into_source(self) -> FontSource {
        self.0
    }
}

impl DescriptorProgram for CidCffProgram {
    fn read(context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        context.required(b"FontFile3")
    }
    fn into_source(self) -> FontSource {
        self.0
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

impl FromPdfObject for TrueTypeProgram {
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
        let data = match subtype.as_deref().unwrap_or(b"OpenType") {
            b"Type1C" | b"CIDFontType0C" => {
                Bytes::from(build_cff_font(context.stream().raw_data())?)
            }
            b"OpenType" => context.stream().shared_data(),
            other => {
                return Err(FontError::UnsupportedFontSubtype {
                    subtype: String::from_utf8_lossy(other).into_owned(),
                }
                .into());
            }
        };
        Ok(Self(FontSource::Memory {
            data,
            format: FontProgramFormat::OpenTypeCff,
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

    struct ProgramFixture<P>(P);

    impl<P: DescriptorProgram> FromPdfObject for ProgramFixture<P> {
        fn from_pdf_object(
            context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
        ) -> ReadResult<Self> {
            P::read(&mut context.dictionary()?).map(Self)
        }
    }
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
            .read::<ProgramFixture<CidCffProgram>>(&ObjectVariant::Dictionary(descriptor))
            .expect("the raw CID-CFF source should parse");
        let (data, format) = memory_source(source.0.into_source());

        assert_eq!(format, FontProgramFormat::CidCff);
        assert_eq!(data.as_ref(), b"raw-cff");
    }

    #[test]
    fn cid_open_type_remains_open_type() {
        let descriptor = descriptor_with_cff(b"OpenType", b"open-type");
        let source = ObjectReader::new(PassthroughResolver)
            .read::<ProgramFixture<CidCffProgram>>(&ObjectVariant::Dictionary(descriptor))
            .expect("the OpenType source should parse");
        let (data, format) = memory_source(source.0.into_source());

        assert_eq!(format, FontProgramFormat::OpenTypeCff);
        assert_eq!(data.as_ref(), b"open-type");
    }

    #[test]
    fn simple_type1c_is_still_wrapped_as_open_type() {
        let descriptor = descriptor_with_cff(b"Type1C", b"raw-cff");
        let source = ObjectReader::new(PassthroughResolver)
            .read::<ProgramFixture<Type1Program>>(&ObjectVariant::Dictionary(descriptor))
            .expect("the simple CFF source should be wrapped");
        let (data, format) = memory_source(source.0.into_source());

        assert_eq!(format, FontProgramFormat::OpenTypeCff);
        assert_ne!(data.as_ref(), b"raw-cff");
    }
}
