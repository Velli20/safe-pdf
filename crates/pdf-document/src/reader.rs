use std::collections::BTreeMap;

use crate::decryption::DocumentDecryptor;
use crate::diagnostic::{PdfReadDiagnostic, PdfReadDiagnosticKind};
use crate::document::PdfDocument;
use crate::encryption::EncryptDictionary;
use crate::error::PdfReaderError;
use crate::object_loader::ObjectLoader;
use crate::page::PdfPage;
use crate::pages::PdfPages;
use crate::report::PdfReadReport;
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_object::object_id::PdfObjectId;
use pdf_object::object_lookup::ObjectLookupExt;
use pdf_object::object_resolver::{ObjectResolver, PassthroughResolver};
use pdf_object::{
    cross_reference_table::{CrossReferenceEntry, CrossReferenceTable},
    error::ObjectError,
    object_variant::ObjectVariant,
    trailer::Trailer,
};
use pdf_parser::parser::PdfParser;
use pdf_resources::object_reader::{ReadCycleTracker, ReadFromDictionary};
use pdf_resources::resource_cache::DefaultResourceCache;

const EMPTY_PASSWORD: &[u8] = b"";

/// Reads PDF documents from byte slices.
#[derive(Default)]
pub struct PdfReader;

impl PdfReader {
    /// Reads a PDF and returns both the document and recoverable read diagnostics.
    pub fn read_with_report(
        &self,
        input: &[u8],
        password: Option<&[u8]>,
    ) -> Result<PdfReadReport, PdfReaderError> {
        let mut parser = PdfParser::from(input);
        validate_header(&mut parser)?;

        let CrossReferenceTable {
            entries,
            mut trailer,
        } = parser.build_xref_table()?;
        let mut diagnostics = Vec::new();
        let encryption = EncryptionContext::from_trailer(
            &mut trailer,
            &entries,
            &mut parser,
            password.unwrap_or(EMPTY_PASSWORD),
            &mut diagnostics,
        )?;
        let mut objects =
            ObjectLoader::new(&entries, &mut parser, encryption, &mut diagnostics).load()?;
        let document = PdfDocument {
            pages: extract_page_tree(&trailer, &mut objects)?,
        };

        Ok(PdfReadReport::new(document, diagnostics))
    }

    /// Reads a PDF and discards recoverable diagnostics.
    pub fn read_from_bytes(
        &self,
        input: &[u8],
        password: Option<&[u8]>,
    ) -> Result<PdfDocument, PdfReaderError> {
        self.read_with_report(input, password)
            .map(PdfReadReport::into_document)
    }
}

/// Validates the optional header near the start of a PDF.
fn validate_header(parser: &mut PdfParser) -> Result<(), PdfReaderError> {
    if let Some(version) = parser.parse_header_in_opening_bytes()?
        && version.major() > 2
    {
        return Err(PdfReaderError::UnsupportedVersion(
            version.major(),
            version.minor(),
        ));
    }
    Ok(())
}

/// Holds decryption state needed while loading objects.
pub(crate) struct EncryptionContext {
    decryptor: Option<DocumentDecryptor>,
    dictionary_object_number: Option<usize>,
}

impl EncryptionContext {
    /// Creates decryption state from the trailer's optional encryption entry.
    fn from_trailer(
        trailer: &mut Trailer,
        entries: &BTreeMap<usize, CrossReferenceEntry>,
        parser: &mut PdfParser,
        password: &[u8],
        diagnostics: &mut Vec<PdfReadDiagnostic>,
    ) -> Result<Self, PdfReaderError> {
        let Some(encrypt_reference) = trailer.dictionary.take("Encrypt") else {
            return Ok(Self {
                decryptor: None,
                dictionary_object_number: None,
            });
        };
        let dictionary_object_number = encrypt_reference.try_object_number().ok();
        let encryption = match load_encrypt_dictionary(encrypt_reference, entries, parser) {
            Ok(encryption) => encryption,
            Err(error) if error.is_recoverable_optional_object_error() => {
                diagnostics.push(PdfReadDiagnostic::new(
                    PdfReadDiagnosticKind::MalformedEncryption,
                    None,
                    dictionary_object_number.map(object_id),
                    error,
                ));
                return Ok(Self {
                    decryptor: None,
                    dictionary_object_number: None,
                });
            }
            Err(error) => return Err(error),
        };
        let document_id = extract_document_id(trailer)?;
        let decryptor = DocumentDecryptor::new(&encryption, document_id, password)
            .map_err(PdfReaderError::from_decryption_setup)?;
        Ok(Self {
            decryptor: Some(decryptor),
            dictionary_object_number,
        })
    }

    /// Returns the decryptor unless the object is the encryption dictionary itself.
    pub(crate) fn decryptor_for(&self, object: Option<PdfObjectId>) -> Option<&DocumentDecryptor> {
        let object_number = object.map(|identifier| identifier.number);
        (object_number != self.dictionary_object_number)
            .then_some(self.decryptor.as_ref())
            .flatten()
    }
}

/// Creates an identifier for an object whose generation is not available in xref context.
pub(crate) fn object_id(number: usize) -> PdfObjectId {
    PdfObjectId {
        number,
        generation: 0,
    }
}

/// Resolves the page tree rooted at the trailer catalog.
fn extract_page_tree(
    trailer: &Trailer,
    objects: &mut dyn ObjectResolver,
) -> Result<Vec<PdfPage>, PdfReaderError> {
    let catalog = trailer.dictionary.required_dictionary("Root", objects)?;
    let pages = catalog.required_dictionary("Pages", objects)?;
    let mut cache = DefaultResourceCache::default();
    let mut cycle_tracker = ReadCycleTracker::default();
    let mut content_stream_ids = ContentStreamIdAllocator::new();
    Ok(PdfPages::from_dictionary(
        pages,
        objects,
        &mut cache,
        &mut cycle_tracker,
        &mut content_stream_ids,
    )?
    .unwrap_or_default())
}

/// Resolves and parses the trailer's encryption dictionary without decrypting it.
fn load_encrypt_dictionary(
    encrypt_reference: ObjectVariant,
    entries: &BTreeMap<usize, CrossReferenceEntry>,
    parser: &mut PdfParser,
) -> Result<EncryptDictionary, PdfReaderError> {
    let object = match encrypt_reference {
        ObjectVariant::Reference(object_number) => {
            let entry =
                entries
                    .get(&object_number)
                    .ok_or(ObjectError::FailedResolveObjectReference {
                        obj_num: object_number,
                    })?;
            let byte_offset =
                entry
                    .byte_offset()
                    .ok_or(ObjectError::FailedResolveObjectReference {
                        obj_num: object_number,
                    })?;
            parser.parse_object_at(byte_offset, &PassthroughResolver)?
        }
        object => object,
    };
    let dictionary = object.try_dictionary(&PassthroughResolver)?;
    EncryptDictionary::from_dictionary(dictionary, &PassthroughResolver)
}

/// Extracts the first trailer document identifier used for encryption key derivation.
fn extract_document_id(trailer: &Trailer) -> Result<&[u8], PdfReaderError> {
    let identifier = trailer
        .dictionary
        .required_array("ID", &PassthroughResolver)?
        .first()
        .ok_or(PdfReaderError::MissingDocumentId)?;
    Ok(identifier.try_bytes(&PassthroughResolver)?)
}
