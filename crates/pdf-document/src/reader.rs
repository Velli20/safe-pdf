use std::collections::{BTreeMap, HashMap};

use crate::decryption::{DecryptionError, DocumentDecryptor};
use crate::document::PdfDocument;
use crate::object_stream::read_object_stream;
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
use pdf_page::page::PdfPage;
use pdf_page::pages::{PdfPages, PdfPagesError};
use pdf_page::resource::Resource;
use pdf_parser::{error::ParserError, header::HeaderError, parser::PdfParser};
use thiserror::Error;

use crate::encryption::{EncryptDictionary, EncryptionError};

/// Errors that can occur while reading a PDF document.
#[derive(Debug, Error)]
pub enum PdfReaderError {
    #[error("missing trailer")]
    MissingTrailer,
    #[error("unexpected reference object at offset {offset}")]
    UnexpectedReference { offset: usize },
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("{0}")]
    PdfPagesError(#[from] PdfPagesError),
    #[error("{0}")]
    ParserError(#[from] ParserError),
    #[error("Error parsing PDF header: {0}")]
    HeaderError(#[from] HeaderError),
    #[error("unsupported PDF version: {0}.{1}")]
    UnsupportedVersion(u8, u8),
    #[error("invalid cross-reference table at offset {offset}")]
    InvalidXrefAtOffset { offset: usize },
    #[error("encryption error: {0}")]
    EncryptionError(#[from] EncryptionError),
    #[error("decryption error: {0}")]
    DecryptionError(#[from] DecryptionError),
    #[error("missing document ID required for encryption")]
    MissingDocumentId,
    #[error(
        "failed to resolve {count} object(s) after {iterations} iteration(s); \
         first unresolved at byte offset {first_offset}"
    )]
    UnresolvedObjects {
        count: usize,
        iterations: usize,
        first_offset: usize,
    },
}

#[derive(Default)]
pub struct PdfReader;

impl PdfReader {
    /// Reads and parses a PDF document from raw bytes.
    ///
    /// This method performs the following steps:
    /// 1. Parses the PDF header and validates the version
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

        // Parse and validate PDF header
        let version = parser.parse_header()?;
        if version.major() != 1 {
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
            // Load the encryption object first (it's unencrypted per PDF spec).
            let encryption = load_encrypt_dictionary(encrypt_ref, &entries, &mut parser)?;

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

    let mut cache: HashMap<usize, Resource> = HashMap::new();
    let pages = PdfPages::from_dictionary(pages_dict, objects, &mut cache)?;
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
    EncryptDictionary::from_dictionary(&encrypt_dict, &PassthroughResolver).map_err(Into::into)
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
            Err(e) => return Err(e),
        }
    }

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
                Err(e) => return Err(e),
            }
        }

        // No progress — the remaining objects are truly unresolvable.
        if still_unresolved.len() == unresolved.len() {
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

    // Pass 2: Unpack type-2 (compressed) entries from object streams.
    // Cache parsed object streams to avoid re-parsing the same stream multiple times.
    let mut parsed_obj_streams: HashMap<usize, Vec<(usize, ObjectVariant)>> = HashMap::new();

    for (&obj_num, entry) in entries {
        let CrossReferenceEntryType::Compressed {
            object_stream_number,
            index_within_stream,
        } = entry.entry_type
        else {
            continue;
        };

        // Parse the object stream if we haven't already
        if let std::collections::hash_map::Entry::Vacant(e) =
            parsed_obj_streams.entry(object_stream_number)
        {
            let stream_obj = objects
                .get(object_stream_number)
                .ok_or(ObjectError::FailedResolveObjectReference {
                    obj_num: object_stream_number,
                })?
                .try_stream(&objects)?;

            let unpacked = read_object_stream(stream_obj, &objects)?;
            e.insert(unpacked);
        }

        // Extract the object at the specified index and insert it.
        // Use `insert_compressed` to overwrite: the object-stream version is authoritative.
        if let Some(cached) = parsed_obj_streams.get(&object_stream_number)
            && let Some((_cached_num, obj)) = cached.get(index_within_stream)
        {
            objects.insert_compressed(obj_num, obj.clone());
        }
    }

    Ok(objects)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to format a standard xref entry (20 bytes)
    fn format_xref_entry(offset: usize, generation: u16, used: bool) -> String {
        let kind = if used { 'n' } else { 'f' };
        // Ensure 20 bytes: 10 digit offset, space, 5 digit gen, space, kind, space, newline
        // Total: 10 + 1 + 5 + 1 + 1 + 1 + 1 = 20
        format!("{:010} {:05} {} \n", offset, generation, kind)
    }

    #[test]
    fn test_encrypted_document_detection() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        // Object 1: Catalog
        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");

        // Object 2: Encryption dictionary (Standard V2)
        let obj2_offset = data.len();
        data.extend_from_slice(b"2 0 obj\n");
        data.extend_from_slice(b"<< /Filter /Standard /V 2 /R 3 /Length 128 ");
        // O and U are 32-byte hex strings (filled with zeros for test)
        data.extend_from_slice(b"/O <00000000000000000000000000000000");
        data.extend_from_slice(b"00000000000000000000000000000000> ");
        data.extend_from_slice(b"/U <00000000000000000000000000000000");
        data.extend_from_slice(b"00000000000000000000000000000000> ");
        data.extend_from_slice(b"/P -1 >>\n");
        data.extend_from_slice(b"endobj\n");

        // Xref table
        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 3\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());

        // Trailer with Encrypt entry
        data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R /Encrypt 2 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let reader = PdfReader;
        let result = reader.read_from_bytes(&data, None);

        // Should return an EncryptedDocument error
        assert!(result.is_err(), "Should detect encrypted document");
    }

    #[test]
    fn test_encrypted_document_v4_aes() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        // Object 1: Catalog
        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");

        // Object 2: Encryption dictionary (Standard V4 - AES)
        let obj2_offset = data.len();
        data.extend_from_slice(b"2 0 obj\n");
        data.extend_from_slice(b"<< /Filter /Standard /V 4 /R 4 /Length 128 ");
        data.extend_from_slice(b"/O <00000000000000000000000000000000");
        data.extend_from_slice(b"00000000000000000000000000000000> ");
        data.extend_from_slice(b"/U <00000000000000000000000000000000");
        data.extend_from_slice(b"00000000000000000000000000000000> ");
        data.extend_from_slice(b"/P -1 /EncryptMetadata false >>\n");
        data.extend_from_slice(b"endobj\n");

        // Xref table
        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 3\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());

        // Trailer with Encrypt entry
        data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R /Encrypt 2 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let reader = PdfReader;
        let result = reader.read_from_bytes(&data, None);

        // Should return an EncryptedDocument error
        assert!(result.is_err(), "Should detect encrypted document");
    }

    #[test]
    fn test_unencrypted_document_loads_normally() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        // Object 1: Catalog
        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        // Object 2: Pages
        let obj2_offset = data.len();
        data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

        // Xref table
        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 3\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());

        // Trailer WITHOUT Encrypt entry
        data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let reader = PdfReader;
        let result = reader.read_from_bytes(&data, None);

        // Should load successfully (no encryption)
        assert!(
            result.is_ok(),
            "Unencrypted document should load: {:?}",
            result.err()
        );

        let doc = result.unwrap();
        assert_eq!(doc.page_count(), 0);
    }

    #[test]
    fn test_stream_with_indirect_length_resolves() {
        // Object 4 is a stream whose /Length is an indirect reference to object 3.
        // Since entries are processed in reverse key order (4, 3, 2, 1), object 4
        // will be deferred on the first pass because object 3 isn't loaded yet.
        // The retry loop should resolve it.
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        // Object 3: the stream length value (5)
        let obj3_offset = data.len();
        data.extend_from_slice(b"3 0 obj\n5\nendobj\n");

        // Object 4: a stream with /Length as an indirect reference
        let obj4_offset = data.len();
        data.extend_from_slice(b"4 0 obj\n<< /Length 3 0 R >>\nstream\nHello\nendstream\nendobj\n");

        // Object 1: Catalog
        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        // Object 2: Pages
        let obj2_offset = data.len();
        data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

        // Xref table
        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 5\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj3_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());

        // Trailer
        data.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let reader = PdfReader;
        let result = reader.read_from_bytes(&data, None);

        assert!(
            result.is_ok(),
            "Stream with indirect /Length should resolve: {:?}",
            result.err()
        );

        let doc = result.unwrap();
        assert_eq!(doc.page_count(), 0);
    }

    #[test]
    fn test_unresolvable_reference_returns_error() {
        // Object 4 is a stream whose /Length references a non-existent object (99 0 R).
        // The retry loop should detect no progress and return UnresolvedObjects.
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        // Object 4: a stream referencing non-existent object 99 for /Length
        let obj4_offset = data.len();
        data.extend_from_slice(
            b"4 0 obj\n<< /Length 99 0 R >>\nstream\nHello\nendstream\nendobj\n",
        );

        // Object 1: Catalog
        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        // Object 2: Pages
        let obj2_offset = data.len();
        data.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

        // Xref table — object 99 is NOT present
        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 5\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());
        // entry 3 = free (placeholder)
        data.extend_from_slice(format_xref_entry(0, 0, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj4_offset, 0, true).as_bytes());

        // Trailer
        data.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let reader = PdfReader;
        let result = reader.read_from_bytes(&data, None);

        assert!(result.is_err(), "Should fail for unresolvable reference");
        let err_msg = match result {
            Err(e) => e.to_string(),
            Ok(_) => unreachable!(),
        };
        assert!(
            err_msg.contains("failed to resolve"),
            "Expected UnresolvedObjects error, got: {err_msg}"
        );
    }
}
