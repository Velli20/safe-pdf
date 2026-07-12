use std::collections::{BTreeMap, HashMap};

use crate::decryption::{DecryptionError, DocumentDecryptor};
use crate::diagnostic::{PdfReadDiagnostic, PdfReadDiagnosticKind};
use crate::document::PdfDocument;
use crate::encryption::EncryptDictionary;
use crate::error::PdfReaderError;
use crate::object_stream::{CompressedObject, read_object_stream};
use crate::page::PdfPage;
use crate::pages::PdfPages;
use crate::report::PdfReadReport;
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_object::object_id::PdfObjectId;
use pdf_object::object_lookup::ObjectLookupExt;
use pdf_object::object_resolver::{ObjectResolver, PassthroughResolver};
use pdf_object::{
    cross_reference_table::{CrossReferenceEntry, CrossReferenceEntryType, CrossReferenceTable},
    error::ObjectError,
    object_variant::ObjectVariant,
    trailer::Trailer,
};
use pdf_object_collection::object_collection::ObjectCollection;
use pdf_parser::error::ParserError;
use pdf_parser::parser::PdfParser;
use pdf_resources::object_reader::{ReadCycleTracker, ReadFromDictionary};
use pdf_resources::resource_cache::DefaultResourceCache;

const MAX_RESOLVE_ITERATIONS: usize = 16;
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
        let objects =
            ObjectLoader::new(&entries, &mut parser, encryption, &mut diagnostics).load()?;
        let document = PdfDocument {
            pages: extract_page_tree(&trailer, &mut objects.into_resolver())?,
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
struct EncryptionContext {
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
            Err(error) if is_recoverable_optional_object_error(&error) => {
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
        let decryptor = DocumentDecryptor::new(&encryption, &document_id, password)
            .map_err(PdfReaderError::from_decryption_setup)?;
        Ok(Self {
            decryptor: Some(decryptor),
            dictionary_object_number,
        })
    }

    /// Returns the decryptor unless the object is the encryption dictionary itself.
    fn decryptor_for(&self, object: Option<PdfObjectId>) -> Option<&DocumentDecryptor> {
        let object_number = object.map(|identifier| identifier.number);
        (object_number != self.dictionary_object_number)
            .then_some(self.decryptor.as_ref())
            .flatten()
    }
}

/// Loads xref-addressable objects while retaining enough state for retries.
struct ObjectLoader<'a> {
    entries: &'a BTreeMap<usize, CrossReferenceEntry>,
    parser: &'a mut PdfParser<'a>,
    encryption: EncryptionContext,
    objects: ObjectCollection,
    object_streams: HashMap<usize, Vec<CompressedObject>>,
    diagnostics: &'a mut Vec<PdfReadDiagnostic>,
}

impl<'a> ObjectLoader<'a> {
    /// Creates an object loader for one parsed cross-reference table.
    fn new(
        entries: &'a BTreeMap<usize, CrossReferenceEntry>,
        parser: &'a mut PdfParser<'a>,
        encryption: EncryptionContext,
        diagnostics: &'a mut Vec<PdfReadDiagnostic>,
    ) -> Self {
        Self {
            entries,
            parser,
            encryption,
            objects: ObjectCollection::default(),
            object_streams: HashMap::new(),
            diagnostics,
        }
    }

    /// Loads normal and compressed objects, returning all successfully loaded objects.
    fn load(mut self) -> Result<LoadedObjects, PdfReaderError> {
        let mut pending = self.load_normal_objects()?;
        self.load_available_compressed_objects(false)?;
        let mut iterations = 0usize;

        while !pending.is_empty() && iterations < MAX_RESOLVE_ITERATIONS {
            iterations = iterations.saturating_add(1);
            let previous_count = pending.len();
            pending = self.retry_pending_objects(pending)?;
            let loaded_compressed = self.load_available_compressed_objects(false)?;
            if pending.len() == previous_count && !loaded_compressed {
                break;
            }
        }

        if let Some(offset) = pending.first().copied() {
            return Err(PdfReaderError::UnresolvedObjects {
                count: pending.len(),
                iterations,
                first_offset: offset,
            });
        }
        self.load_available_compressed_objects(true)?;
        Ok(LoadedObjects(self.objects))
    }

    /// Loads each normal xref entry once and returns references that need a later retry.
    fn load_normal_objects(&mut self) -> Result<Vec<usize>, PdfReaderError> {
        let mut pending = Vec::new();
        for entry in self.entries.values().rev() {
            let CrossReferenceEntryType::Normal { byte_offset, .. } = entry.entry_type else {
                continue;
            };
            if byte_offset == 0 {
                continue;
            }
            let result = self.load_object(byte_offset);
            self.handle_load_result(byte_offset, &mut pending, result)?;
        }
        Ok(pending)
    }

    /// Retries object offsets that previously referred to unavailable objects.
    fn retry_pending_objects(&mut self, pending: Vec<usize>) -> Result<Vec<usize>, PdfReaderError> {
        let mut still_pending = Vec::new();
        for byte_offset in pending {
            let result = self.load_object(byte_offset);
            self.handle_load_result(byte_offset, &mut still_pending, result)?;
        }
        Ok(still_pending)
    }

    /// Classifies a single object load result according to the best-effort policy.
    fn handle_load_result(
        &mut self,
        byte_offset: usize,
        pending: &mut Vec<usize>,
        result: Result<(), ObjectLoadError>,
    ) -> Result<(), PdfReaderError> {
        match result {
            Ok(()) => Ok(()),
            Err(ObjectLoadError::Reader(error)) if is_unresolved_reference(&error) => {
                pending.push(byte_offset);
                Ok(())
            }
            Err(ObjectLoadError::Reader(error)) if is_recoverable_optional_object_error(&error) => {
                self.diagnostics.push(PdfReadDiagnostic::new(
                    PdfReadDiagnosticKind::ObjectParse,
                    Some(byte_offset),
                    None,
                    error,
                ));
                Ok(())
            }
            Err(ObjectLoadError::Decryption(error)) => {
                self.diagnostics.push(PdfReadDiagnostic::new(
                    PdfReadDiagnosticKind::ObjectDecryption,
                    Some(byte_offset),
                    None,
                    error,
                ));
                Ok(())
            }
            Err(ObjectLoadError::Reader(error)) => Err(error),
        }
    }

    /// Parses, decrypts, and inserts one normal indirect object.
    fn load_object(&mut self, byte_offset: usize) -> Result<(), ObjectLoadError> {
        let object = self
            .parser
            .parse_object_at(byte_offset, &self.objects)
            .map_err(PdfReaderError::ParserError)?;
        if matches!(object, ObjectVariant::Reference(_)) {
            return Err(PdfReaderError::UnexpectedReference {
                offset: byte_offset,
            }
            .into());
        }
        let identifier = object_identifier(&object);
        let object = match self.encryption.decryptor_for(identifier) {
            Some(decryptor) => decryptor.decrypt_object(object)?,
            None => object,
        };
        self.objects
            .insert(object)
            .map_err(PdfReaderError::ObjectError)?;
        Ok(())
    }

    /// Loads compressed objects whose containing object streams are available.
    fn load_available_compressed_objects(&mut self, strict: bool) -> Result<bool, PdfReaderError> {
        let mut loaded_any = false;
        for (&object_number, entry) in self.entries {
            let CrossReferenceEntryType::Compressed {
                object_stream_number,
                index_within_stream,
            } = entry.entry_type
            else {
                continue;
            };
            if self.objects.get(object_number).is_some() {
                continue;
            }
            if !self.object_streams.contains_key(&object_stream_number) {
                let Some(stream_object) = self.objects.get(object_stream_number) else {
                    if strict {
                        return Err(ObjectError::FailedResolveObjectReference {
                            obj_num: object_stream_number,
                        }
                        .into());
                    }
                    continue;
                };
                let stream = stream_object.try_stream(&self.objects)?;
                match read_object_stream(stream, &self.objects) {
                    Ok(objects) => {
                        self.object_streams.insert(object_stream_number, objects);
                    }
                    Err(error) if !strict && is_recoverable_optional_object_error(&error) => {
                        self.diagnostics.push(PdfReadDiagnostic::new(
                            PdfReadDiagnosticKind::CompressedObject,
                            None,
                            Some(object_id(object_stream_number)),
                            error,
                        ));
                        continue;
                    }
                    Err(error) => return Err(error),
                }
            }
            if let Some(object) = self
                .object_streams
                .get(&object_stream_number)
                .and_then(|objects| objects.get(index_within_stream))
            {
                if object.number != object_number {
                    self.diagnostics.push(PdfReadDiagnostic::new(
                        PdfReadDiagnosticKind::CompressedObject,
                        None,
                        Some(object_id(object_number)),
                        "xref compressed object number differed from its object stream entry",
                    ));
                }
                self.objects
                    .insert_compressed(object_number, object.value.clone());
                loaded_any = true;
            }
        }
        Ok(loaded_any)
    }
}

/// Wraps the loaded collection so ownership can cross the reader boundary explicitly.
struct LoadedObjects(ObjectCollection);

impl LoadedObjects {
    /// Returns the owned collection as an object resolver.
    fn into_resolver(self) -> ObjectCollection {
        self.0
    }
}

/// Distinguishes reader failures from recoverable object decryption failures.
enum ObjectLoadError {
    /// An ordinary reader failure.
    Reader(PdfReaderError),
    /// A failure while decrypting one object.
    Decryption(DecryptionError),
}

impl From<PdfReaderError> for ObjectLoadError {
    /// Wraps a reader error produced while loading an object.
    fn from(error: PdfReaderError) -> Self {
        Self::Reader(error)
    }
}

impl From<DecryptionError> for ObjectLoadError {
    /// Wraps an object decryption error.
    fn from(error: DecryptionError) -> Self {
        Self::Decryption(error)
    }
}

/// Extracts a named object identifier when an object carries one.
fn object_identifier(object: &ObjectVariant) -> Option<PdfObjectId> {
    match object {
        ObjectVariant::IndirectObject(indirect) => Some(PdfObjectId {
            number: indirect.object_number,
            generation: indirect.generation_number,
        }),
        ObjectVariant::Stream(stream) => Some(PdfObjectId {
            number: stream.object_number,
            generation: stream.generation_number,
        }),
        _ => None,
    }
}

/// Creates an identifier for an object whose generation is not available in xref context.
fn object_id(number: usize) -> PdfObjectId {
    PdfObjectId {
        number,
        generation: 0,
    }
}

/// Returns whether an object failure should be retried after more objects load.
fn is_unresolved_reference(error: &PdfReaderError) -> bool {
    matches!(
        error,
        PdfReaderError::ParserError(ParserError::ObjectError(
            ObjectError::FailedResolveObjectReference { .. }
        ))
    )
}

/// Returns whether a bulk-loaded object may be skipped without preventing a usable document.
fn is_recoverable_optional_object_error(error: &PdfReaderError) -> bool {
    matches!(
        error,
        PdfReaderError::ParserError(_)
            | PdfReaderError::ObjectError(ObjectError::DecompressionError(_))
    )
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
    let dictionary = match encrypt_reference {
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
            dictionary_from_object(parser.parse_object_at(byte_offset, &PassthroughResolver)?)?
        }
        ObjectVariant::Dictionary(dictionary) => dictionary,
        object => {
            return Err(ObjectError::FailedResolveDictionaryObject {
                resolved_type: object.name(),
            }
            .into());
        }
    };
    EncryptDictionary::from_dictionary(&dictionary, &PassthroughResolver)
}

/// Extracts a dictionary from an object used as an encryption dictionary.
fn dictionary_from_object(
    object: ObjectVariant,
) -> Result<Box<pdf_object::dictionary::Dictionary>, PdfReaderError> {
    match object {
        ObjectVariant::Dictionary(dictionary) => Ok(dictionary),
        ObjectVariant::IndirectObject(indirect) => match indirect.object {
            Some(ObjectVariant::Dictionary(dictionary)) => Ok(dictionary),
            _ => Err(ObjectError::FailedResolveDictionaryObject {
                resolved_type: "IndirectObject",
            }
            .into()),
        },
        object => Err(ObjectError::FailedResolveDictionaryObject {
            resolved_type: object.name(),
        }
        .into()),
    }
}

/// Extracts the first trailer document identifier used for encryption key derivation.
fn extract_document_id(trailer: &Trailer) -> Result<Vec<u8>, PdfReaderError> {
    let identifier = trailer
        .dictionary
        .required_array("ID", &PassthroughResolver)?
        .first()
        .ok_or(PdfReaderError::MissingDocumentId)?;
    Ok(identifier.try_bytes(&PassthroughResolver)?.to_vec())
}
