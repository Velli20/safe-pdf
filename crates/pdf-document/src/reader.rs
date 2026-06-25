use std::collections::{BTreeMap, HashMap};

use crate::decryption::DocumentDecryptor;
use crate::document::PdfDocument;
use crate::error::PdfReaderError;
use crate::object_stream::read_object_stream;
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_object::indirect_object::IndirectObject;
use pdf_object::object_resolver::{ObjectResolver, PassthroughResolver};
use pdf_object::{
    cross_reference_table::{CrossReferenceEntry, CrossReferenceEntryType, CrossReferenceTable},
    error::ObjectError,
    object_variant::ObjectVariant,
    stream::StreamObject,
    trailer::Trailer,
};
use pdf_object_collection::object_collection::ObjectCollection;
use pdf_page::object_reader::{ReadCycleTracker, ReadFromDictionary};
use pdf_page::page::PdfPage;
use pdf_page::pages::PdfPages;
use pdf_page::resource_cache::DefaultResourceCache;
use pdf_parser::error::ParserError;
use pdf_parser::parser::PdfParser;

use crate::encryption::EncryptDictionary;

#[derive(Default)]
pub struct PdfReader;

impl PdfReader {
    /// Reads and parses a PDF document from raw bytes.
    ///
    /// This method performs the following steps:
    /// 1. Parses the PDF header when present and validates the version
    /// 2. Builds the cross-reference index to locate all objects
    /// 3. Checks for encryption and resolves the Encrypt dictionary first
    /// 4. Loads all objects referenced in the xref table
    /// 5. Extracts the document catalog and page tree
    ///
    /// # Parameters
    ///
    /// - `input`: Raw PDF file bytes
    /// - `password`: The document password (user or owner password)
    ///
    /// # Returns
    ///
    /// Returns a `PdfDocument` containing the parsed objects and page structure.
    pub fn read_from_bytes(
        &self,
        input: &[u8],
        password: Option<&[u8]>,
    ) -> Result<PdfDocument, PdfReaderError> {
        let mut parser = PdfParser::from(input);

        // Parse and validate the PDF header when it is present near the start of the file.
        if let Some(version) = parser.parse_header_in_opening_bytes()?
            && version.major() > 2
        {
            return Err(PdfReaderError::UnsupportedVersion(
                version.major(),
                version.minor(),
            ));
        }

        // Build the cross-reference index
        let CrossReferenceTable {
            entries,
            mut trailer,
        } = parser.build_xref_table()?;

        // Check for encryption and handle it before loading other objects.
        let decryptor = if let Some(encrypt_ref) = trailer.dictionary.take("Encrypt") {
            match load_encrypt_dictionary(encrypt_ref, &entries, &mut parser) {
                Ok(encryption) => {
                    // Get the document ID from the trailer (required for encryption)
                    let document_id = extract_document_id(&trailer)?;

                    const EMPTY_PASSWORD: &[u8] = b"";

                    // Create the decryptor by authenticating with the password
                    let decryptor = DocumentDecryptor::new(
                        &encryption,
                        &document_id,
                        password.unwrap_or(EMPTY_PASSWORD),
                    )?;

                    Some(decryptor)
                }
                Err(error) if is_recoverable_optional_object_error(&error) => None,
                Err(error) => return Err(error),
            }
        } else {
            None
        };

        // Load all objects from the xref table, decrypting streams if needed
        let mut objects = load_objects_with_decryption(&entries, &mut parser, decryptor.as_ref())?;

        // Extract catalog and page tree
        let pages = extract_page_tree(&trailer, &mut objects)?;

        Ok(PdfDocument { pages })
    }
}

/// Extracts the page tree from the document catalog using a shared resource cache.
///
/// Follows the chain: Trailer → /Root (Catalog) → /Pages (Page Tree).
/// A `ResourceCache` is threaded through the traversal so that resources referenced
/// by the same PDF object number are parsed once and shared via `Rc`.
fn extract_page_tree(
    trailer: &Trailer,
    objects: &mut dyn ObjectResolver,
) -> Result<Vec<PdfPage>, PdfReaderError> {
    // Get the document catalog via the /Root entry in the trailer
    let catalog = trailer
        .dictionary
        .get_or_err("Root")?
        .try_dictionary(objects)?;

    // Get the page tree via the /Pages entry in the catalog
    let pages_dict = catalog.get_or_err("Pages")?.try_dictionary(objects)?;

    let mut cache = DefaultResourceCache::default();
    let mut cycle_tracker = ReadCycleTracker::default();
    let mut content_stream_ids = ContentStreamIdAllocator::new();
    let pages = PdfPages::from_dictionary(
        pages_dict,
        objects,
        &mut cache,
        &mut cycle_tracker,
        &mut content_stream_ids,
    )?
    .unwrap_or_default();
    Ok(pages)
}

/// Loads and parses the encryption dictionary from the PDF.
///
/// According to PDF specification, the encryption dictionary itself is NOT encrypted.
/// This allows a reader to parse the encryption parameters needed to decrypt
/// the rest of the document.
///
/// The `/Encrypt` entry in the trailer is typically an indirect reference, so we need
/// to locate and parse that object first before we can understand how to decrypt
/// other objects.
///
/// # Parameters
///
/// - `encrypt_ref`: The `/Encrypt` entry from the trailer (usually an indirect reference).
/// - `entries`: The cross-reference table entries for locating objects.
/// - `parser`: The PDF parser for reading object data.
///
/// # Returns
///
/// Returns an `EncryptDictionary` containing the encryption parameters.
fn load_encrypt_dictionary(
    encrypt_ref: ObjectVariant,
    entries: &BTreeMap<usize, CrossReferenceEntry>,
    parser: &mut PdfParser,
) -> Result<EncryptDictionary, PdfReaderError> {
    // If the Encrypt entry is an indirect reference, we need to load that object.
    let encrypt_dict = match encrypt_ref {
        ObjectVariant::Reference(obj_num) => {
            // Look up the object in the xref table
            let entry = entries
                .get(&obj_num)
                .ok_or(ObjectError::FailedResolveObjectReference { obj_num })?;

            let byte_offset = entry
                .byte_offset()
                .ok_or(ObjectError::FailedResolveObjectReference { obj_num })?;

            // Parse the encryption object at the specified offset
            let object = parser.parse_object_at(byte_offset, &PassthroughResolver)?;

            // Extract the dictionary from the parsed object
            match object {
                ObjectVariant::Dictionary(dict) => dict,
                ObjectVariant::IndirectObject(indirect) => match indirect.object {
                    Some(ObjectVariant::Dictionary(dict)) => dict,
                    _ => {
                        return Err(ObjectError::FailedResolveDictionaryObject {
                            resolved_type: "IndirectObject",
                        }
                        .into());
                    }
                },
                other => {
                    return Err(ObjectError::FailedResolveDictionaryObject {
                        resolved_type: other.name(),
                    }
                    .into());
                }
            }
        }
        ObjectVariant::Dictionary(dict) => dict,
        other => {
            return Err(ObjectError::FailedResolveDictionaryObject {
                resolved_type: other.name(),
            }
            .into());
        }
    };

    // Parse the encryption dictionary
    EncryptDictionary::from_dictionary(&encrypt_dict, &PassthroughResolver)
}

/// Extracts the document ID from the trailer's /ID array.
///
/// The /ID entry is an array of two byte strings that uniquely identify the document.
/// The first element is used for encryption key derivation.
///
/// # Parameters
///
/// - `trailer`: The PDF trailer containing the /ID entry.
///
/// # Returns
///
/// The first element of the /ID array as a byte vector.
fn extract_document_id(trailer: &Trailer) -> Result<Vec<u8>, PdfReaderError> {
    // Get the first element of the /ID array
    let first_element = trailer
        .dictionary
        .get_or_err("ID")?
        .try_array(&PassthroughResolver)?
        .first()
        .ok_or(PdfReaderError::MissingDocumentId)?;

    Ok(first_element.try_bytes(&PassthroughResolver)?.to_vec())
}

/// Loads all objects referenced in the cross-reference table with optional decryption.
///
/// This function extends `load_objects` by decrypting stream data when a decryptor
/// is provided. Only streams are decrypted; strings within dictionaries are decrypted
/// separately during object resolution.
///
/// # Parameters
///
/// - `entries`: The cross-reference table entries.
/// - `parser`: The PDF parser for reading object data.
/// - `decryptor`: Optional decryptor for encrypted documents.
///
/// # Returns
///
/// Returns an `ObjectCollection` containing all parsed (and decrypted) objects.
fn load_objects_with_decryption(
    entries: &BTreeMap<usize, CrossReferenceEntry>,
    parser: &mut PdfParser,
    decryptor: Option<&DocumentDecryptor>,
) -> Result<ObjectCollection, PdfReaderError> {
    /// Maximum number of retry iterations for resolving forward references.
    /// Real PDFs rarely need more than 1–2; this is a safety cap.
    const MAX_RESOLVE_ITERATIONS: usize = 16;

    let mut objects = ObjectCollection::default();
    let mut unresolved: Vec<usize> = Vec::new();
    let mut parsed_obj_streams: HashMap<usize, Vec<(usize, ObjectVariant)>> = HashMap::new();

    // Pass 1: Load all type-1 (normal) entries — these are objects at byte offsets,
    // including the object streams themselves.
    for entry in entries.values().rev() {
        let CrossReferenceEntryType::Normal { byte_offset, .. } = entry.entry_type else {
            continue;
        };

        // Offset 0 always points to the PDF header, not a valid indirect object.
        // Some PDF generators emit normal entries with offset 0 for deleted/null objects.
        if byte_offset == 0 {
            continue;
        }

        match try_load_object(byte_offset, parser, &mut objects, decryptor) {
            Ok(()) => {}
            Err(PdfReaderError::ParserError(ParserError::ObjectError(
                ObjectError::FailedResolveObjectReference { .. },
            ))) => {
                unresolved.push(byte_offset);
            }
            Err(e) if is_recoverable_optional_object_error(&e) => {}
            Err(e) => return Err(e),
        }
    }

    load_available_compressed_objects(entries, &mut objects, &mut parsed_obj_streams, false)?;

    // Iteratively retry unresolved objects until convergence or the cap is reached.
    // Each iteration may resolve objects whose dependencies were loaded in a prior
    // iteration, unblocking further progress.
    let mut iterations: usize = 0;
    while !unresolved.is_empty() && iterations < MAX_RESOLVE_ITERATIONS {
        iterations = iterations.saturating_add(1);
        let mut still_unresolved: Vec<usize> = Vec::new();

        for byte_offset in &unresolved {
            match try_load_object(*byte_offset, parser, &mut objects, decryptor) {
                Ok(()) => {}
                Err(PdfReaderError::ParserError(ParserError::ObjectError(
                    ObjectError::FailedResolveObjectReference { .. },
                ))) => {
                    still_unresolved.push(*byte_offset);
                }
                Err(e) if is_recoverable_optional_object_error(&e) => {}
                Err(e) => return Err(e),
            }
        }

        let loaded_compressed = load_available_compressed_objects(
            entries,
            &mut objects,
            &mut parsed_obj_streams,
            false,
        )?;

        // No progress — the remaining objects are truly unresolvable.
        if still_unresolved.len() == unresolved.len() && !loaded_compressed {
            break;
        }

        unresolved = still_unresolved;
    }

    if let Some(&first_offset) = unresolved.first() {
        return Err(PdfReaderError::UnresolvedObjects {
            count: unresolved.len(),
            iterations,
            first_offset,
        });
    }

    load_available_compressed_objects(entries, &mut objects, &mut parsed_obj_streams, true)?;

    Ok(objects)
}

/// Returns whether a bulk-loaded object can be skipped until a required reference proves otherwise.
fn is_recoverable_optional_object_error(error: &PdfReaderError) -> bool {
    matches!(
        error,
        PdfReaderError::ParserError(_)
            | PdfReaderError::ObjectError(ObjectError::DecompressionError(_))
    )
}

/// Unpacks compressed xref entries whose object streams are already available.
///
/// Early calls run in best-effort mode because object streams can themselves be
/// blocked behind normal-object dependencies. The final call is strict and keeps
/// missing object streams as hard errors.
fn load_available_compressed_objects(
    entries: &BTreeMap<usize, CrossReferenceEntry>,
    objects: &mut ObjectCollection,
    parsed_obj_streams: &mut HashMap<usize, Vec<(usize, ObjectVariant)>>,
    strict: bool,
) -> Result<bool, PdfReaderError> {
    let mut loaded_any = false;

    for (&obj_num, entry) in entries {
        let CrossReferenceEntryType::Compressed {
            object_stream_number,
            index_within_stream,
        } = entry.entry_type
        else {
            continue;
        };

        if objects.get(obj_num).is_some() {
            continue;
        }

        // Parse the object stream if we haven't already
        if let std::collections::hash_map::Entry::Vacant(e) =
            parsed_obj_streams.entry(object_stream_number)
        {
            let Some(stream_obj) = objects.get(object_stream_number) else {
                if strict {
                    return Err(ObjectError::FailedResolveObjectReference {
                        obj_num: object_stream_number,
                    }
                    .into());
                }
                continue;
            };

            let stream_obj = stream_obj.try_stream(&*objects)?;

            let unpacked = read_object_stream(stream_obj, &*objects)?;
            e.insert(unpacked);
        }

        // Extract the object at the specified index and insert it.
        // Use `insert_compressed` to overwrite: the object-stream version is authoritative.
        if let Some(cached) = parsed_obj_streams.get(&object_stream_number)
            && let Some((_cached_num, obj)) = cached.get(index_within_stream)
        {
            objects.insert_compressed(obj_num, obj.clone());
            loaded_any = true;
        }
    }

    Ok(loaded_any)
}

/// Parses a single object at `byte_offset`, validates it, optionally decrypts it,
/// and inserts it into the collection.
fn try_load_object(
    byte_offset: usize,
    parser: &mut PdfParser,
    objects: &mut ObjectCollection,
    decryptor: Option<&DocumentDecryptor>,
) -> Result<(), PdfReaderError> {
    let object = parser.parse_object_at(byte_offset, objects)?;

    if matches!(object, ObjectVariant::Reference(_)) {
        return Err(PdfReaderError::UnexpectedReference {
            offset: byte_offset,
        });
    }

    let object = if let Some(decryptor) = decryptor {
        decrypt_object(object, decryptor)?
    } else {
        object
    };

    objects.insert(object)?;
    Ok(())
}

/// Decrypts an object's stream data if applicable.
///
/// Only stream objects are decrypted. The object number and generation number
/// are used to derive the object-specific encryption key.
fn decrypt_object(
    object: ObjectVariant,
    decryptor: &DocumentDecryptor,
) -> Result<ObjectVariant, PdfReaderError> {
    match object {
        ObjectVariant::IndirectObject(indirect) => {
            // Check if the inner object is a stream
            if let Some(ObjectVariant::Stream(stream)) = indirect.object {
                let decrypted_stream = decrypt_stream_object(&stream, decryptor)?;
                // Create a new IndirectObject with the decrypted stream
                let new_indirect = IndirectObject::new(
                    indirect.object_number,
                    indirect.generation_number,
                    Some(ObjectVariant::Stream(decrypted_stream)),
                );
                return Ok(ObjectVariant::IndirectObject(Box::new(new_indirect)));
            }
            // Non-stream indirect objects pass through unchanged
            Ok(ObjectVariant::IndirectObject(indirect))
        }
        ObjectVariant::Stream(stream) => {
            let decrypted = decrypt_stream_object(&stream, decryptor)?;
            Ok(ObjectVariant::Stream(decrypted))
        }
        // Other objects pass through unchanged.
        other => Ok(other),
    }
}

/// Decrypts a stream object's data.
fn decrypt_stream_object(
    stream: &StreamObject,
    decryptor: &DocumentDecryptor,
) -> Result<StreamObject, PdfReaderError> {
    // Get the raw (encrypted) stream data
    let encrypted_data = stream.raw_data();

    // Decrypt the stream data using the object's number and generation
    let decrypted_data = decryptor.decrypt_stream(
        stream.object_number,
        stream.generation_number,
        encrypted_data,
    )?;

    // Create a new stream object with decrypted data
    Ok(StreamObject::new(
        stream.object_number,
        stream.generation_number,
        stream.dictionary.clone(),
        decrypted_data,
    ))
}
