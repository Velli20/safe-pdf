use std::collections::{BTreeMap, HashMap, HashSet};

use crate::decryption::{DecryptionError, DocumentDecryptor};
use crate::document::PdfDocument;
use crate::object_stream::read_object_stream;
use pdf_object::indirect_object::IndirectObject;
use pdf_object::object_resolver::{ObjectResolver, UnimplementedResolver};
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
use pdf_page::resource_cache::ResourceCache;
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
        &mut self,
        input: &[u8],
        password: Option<&[u8]>,
    ) -> Result<PdfDocument, PdfReaderError> {
        self.read_from_bytes_internal(input, password)
    }

    /// Internal implementation for reading PDF documents with optional password.
    fn read_from_bytes_internal(
        &mut self,
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
        } = build_xref_index(&mut parser)?;

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

/// Builds the cross-reference index by locating the xref structure at the end of the file.
///
/// Supports both traditional xref tables (PDF 1.0–1.4) and cross-reference streams (PDF 1.5+).
///
/// Strategy:
/// 1. Scan backwards for `startxref` to find the byte offset of the xref section
/// 2. Parse the object at that offset
/// 3. If it's a traditional `xref` table, follow the old path
/// 4. If it's a stream object with `/Type /XRef`, parse as xref stream
/// 5. Follow `/Prev` chain to merge all sections
fn build_xref_index(parser: &mut PdfParser) -> Result<CrossReferenceTable, PdfReaderError> {
    // Find the startxref offset by scanning backwards for "startxref"
    const STARTXREF_KEYWORD: &[u8] = b"startxref";
    let startxref_pos = parser
        .tokenizer
        .input
        .windows(STARTXREF_KEYWORD.len())
        .rposition(|window| window == STARTXREF_KEYWORD)
        .ok_or(PdfReaderError::MissingTrailer)?;

    // Extract the byte offset number after "startxref" keyword
    let offset_start = startxref_pos.saturating_add(STARTXREF_KEYWORD.len());
    let remaining = parser
        .tokenizer
        .input
        .get(offset_start..)
        .ok_or(PdfReaderError::MissingTrailer)?;

    // Skip whitespace, then read digits
    let digits_start = remaining
        .iter()
        .position(|b| b.is_ascii_digit())
        .ok_or(PdfReaderError::MissingTrailer)?;
    let digits_end = remaining
        .get(digits_start..)
        .map(|s| {
            digits_start.saturating_add(
                s.iter()
                    .position(|b| !b.is_ascii_digit())
                    .unwrap_or(s.len()),
            )
        })
        .unwrap_or(digits_start);
    let xref_offset: usize = std::str::from_utf8(
        remaining
            .get(digits_start..digits_end)
            .ok_or(PdfReaderError::MissingTrailer)?,
    )
    .map_err(|_| PdfReaderError::MissingTrailer)?
    .parse()
    .map_err(|_| PdfReaderError::MissingTrailer)?;

    // Follow the xref chain, merging entries from all linked tables
    merge_xref_chain(parser, xref_offset)
}

/// Follows the xref chain via `/Prev` entries and merges all cross-reference tables.
///
/// This handles incremental PDF updates where each update adds a new xref section
/// that references the previous one via the `/Prev` entry in the trailer.
///
/// Supports both traditional xref tables and xref streams at each step in the chain.
fn merge_xref_chain(
    parser: &mut PdfParser,
    start_offset: usize,
) -> Result<CrossReferenceTable, PdfReaderError> {
    let mut entries: BTreeMap<usize, CrossReferenceEntry> = BTreeMap::new();
    let mut visited_offsets = HashSet::new();
    let mut current_offset = start_offset;
    let mut trailer = None;
    loop {
        // Prevent infinite loops from circular references
        if !visited_offsets.insert(current_offset) {
            break;
        }

        // Parse the object at the current offset — could be xref table or xref stream
        let parsed = parser.parse_object_at(current_offset, &UnimplementedResolver)?;

        let xref_table = match parsed {
            ObjectVariant::CrossReferenceTable(table) => table,
            ObjectVariant::IndirectObject(indirect) => {
                // Xref stream: an indirect object wrapping a stream with /Type /XRef
                match indirect.object {
                    Some(ObjectVariant::Stream(ref stream)) => {
                        pdf_parser::cross_reference_stream::parse_xref_stream(
                            stream,
                            &UnimplementedResolver,
                        )?
                    }
                    _ => {
                        return Err(PdfReaderError::InvalidXrefAtOffset {
                            offset: current_offset,
                        });
                    }
                }
            }
            ObjectVariant::Stream(ref stream) => {
                pdf_parser::cross_reference_stream::parse_xref_stream(
                    stream,
                    &UnimplementedResolver,
                )?
            }
            _ => {
                return Err(PdfReaderError::InvalidXrefAtOffset {
                    offset: current_offset,
                });
            }
        };

        // Merge entries: newer entries (already in merged_xref) take precedence
        for (obj_num, entry) in xref_table.entries {
            entries.entry(obj_num).or_insert(entry);
        }

        let prev_value = xref_table.trailer.dictionary.get("Prev").cloned();

        // Select the best trailer: prefer one with a `/Root` entry
        match trailer.as_ref() {
            None => {
                trailer = Some(xref_table.trailer);
            }
            Some(existing) if existing.dictionary.get("Root").is_none() => {
                if xref_table.trailer.dictionary.get("Root").is_some() {
                    trailer = Some(xref_table.trailer);
                }
            }
            _ => {}
        }

        // Follow the chain to the previous xref section
        if let Some(prev_value) = prev_value {
            current_offset = prev_value.try_number::<usize>(&UnimplementedResolver)?;
        } else {
            break;
        }
    }

    let trailer = trailer.ok_or(PdfReaderError::MissingTrailer)?;

    Ok(CrossReferenceTable::new(entries, trailer))
}

#[derive(Default)]
struct ResourceCacheWrapper {
    cache: HashMap<usize, Resource>,
}

impl ResourceCache for ResourceCacheWrapper {
    fn get(&self, obj_num: &usize) -> Option<&Resource> {
        self.cache.get(obj_num)
    }

    fn insert(&mut self, obj_num: usize, resource: Resource) {
        self.cache.insert(obj_num, resource);
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

    let mut cache = ResourceCacheWrapper::default();
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
            let object = parser.parse_object_at(byte_offset, &UnimplementedResolver)?;

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
    EncryptDictionary::from_dictionary(&encrypt_dict, &UnimplementedResolver).map_err(Into::into)
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
        .try_array(&UnimplementedResolver)?
        .first()
        .ok_or(PdfReaderError::MissingDocumentId)?;

    Ok(first_element.try_bytes(&UnimplementedResolver)?.to_vec())
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
    let mut objects = ObjectCollection::default();

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

        let object = parser.parse_object_at(byte_offset, &objects)?;

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

        // Extract the object at the specified index
        if let Some(cached) = parsed_obj_streams.get(&object_stream_number)
            && let Some((_cached_num, obj)) = cached.get(index_within_stream)
        {
            objects.map.insert(obj_num, obj.clone());
        }
    }

    Ok(objects)
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
        stream.filters().cloned(),
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
    fn test_build_xref_index_simple() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        // Object 1: Catalog
        let obj1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");

        // Xref table
        let xref_offset = data.len();
        data.extend_from_slice(b"xref\n0 2\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(obj1_offset, 0, true).as_bytes());

        // Trailer
        data.extend_from_slice(b"trailer\n<< /Size 2 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let mut parser = PdfParser::from(data.as_slice());
        let result = build_xref_index(&mut parser);

        assert!(
            result.is_ok(),
            "Should successfully build xref index: {:?}",
            result.err()
        );
        let table = result.unwrap();

        // Check entries: Includes free object 0 and object 1.
        assert_eq!(
            table.entries.len(),
            2,
            "Should have 2 entries (obj 0 and obj 1)"
        );

        let entry1 = table.entries.get(&1).expect("Obj 1 should exist");
        assert_eq!(entry1.byte_offset(), Some(obj1_offset));

        // Check free entry
        let entry0 = table.entries.get(&0).expect("Obj 0 should exist");
        assert!(entry0.is_free(), "Obj 0 should be free");

        // Check trailer
        let size: i64 = table
            .trailer
            .dictionary
            .get("Size")
            .expect("Size expected")
            .try_number(&UnimplementedResolver)
            .unwrap();
        assert_eq!(size, 2);
    }

    #[test]
    fn test_merge_xref_chain_incremental() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        // --- Revision 1 ---
        // Obj 1 (v1)
        let _obj1_v1_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n(v1)\nendobj\n");
        // Obj 2
        let obj2_offset = data.len();
        data.extend_from_slice(b"2 0 obj\n(obj2)\nendobj\n");

        let xref1_offset = data.len();
        data.extend_from_slice(b"xref\n0 3\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(format_xref_entry(_obj1_v1_offset, 0, true).as_bytes());
        data.extend_from_slice(format_xref_entry(obj2_offset, 0, true).as_bytes());

        data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref1_offset).as_bytes());
        data.extend_from_slice(b"%%EOF\n");

        // --- Revision 2 (Update Obj 1) ---
        // Obj 1 (v2)
        let obj1_v2_offset = data.len();
        data.extend_from_slice(b"1 0 obj\n(v2)\nendobj\n");

        let xref2_offset = data.len();
        data.extend_from_slice(b"xref\n0 1\n"); // dummy head
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        data.extend_from_slice(b"1 1\n"); // Subsection for obj 1
        data.extend_from_slice(format_xref_entry(obj1_v2_offset, 0, true).as_bytes());

        // Trailer points to Prev xref
        data.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R /Prev ");
        data.extend_from_slice(format!("{}", xref1_offset).as_bytes());
        data.extend_from_slice(b" >>\n");
        data.extend_from_slice(b"startxref\n");
        data.extend_from_slice(format!("{}\n", xref2_offset).as_bytes());
        data.extend_from_slice(b"%%EOF");

        let mut parser = PdfParser::from(data.as_slice());

        // Test merge_xref_chain starting from the second xref
        let result = merge_xref_chain(&mut parser, xref2_offset);

        assert!(result.is_ok(), "Should merge xref chain");
        let table = result.unwrap();

        // Check Obj 1 (should be v2)
        let entry1 = table.entries.get(&1).expect("Obj 1 missing");
        assert_eq!(
            entry1.byte_offset(),
            Some(obj1_v2_offset),
            "Obj 1 should point to v2"
        );

        // Check Obj 2 (should be from v1)
        let entry2 = table.entries.get(&2).expect("Obj 2 missing");
        assert_eq!(
            entry2.byte_offset(),
            Some(obj2_offset),
            "Obj 2 should be from v1"
        );
    }

    #[test]
    fn test_merge_xref_circular_protection() {
        let mut data = Vec::new();
        data.extend_from_slice(b"%PDF-1.7\n");

        // Create 2 xrefs pointing to each other
        let _xref1_pos_holder = data.len();

        // Let's just put placeholders.
        // Xref 1 at offset 100
        while data.len() < 100 {
            data.push(b' ');
        }
        let xref1_offset = data.len();
        data.extend_from_slice(b"xref\n0 1\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        // Trailer 1 points to Prev = 200 (xref2)
        data.extend_from_slice(b"trailer\n<< /Prev 200 >>\n");
        // Assuming parser might greedily look for startxref if it treats it as a file trailer
        data.extend_from_slice(b"startxref\n0\n%%EOF\n");

        // Xref 2 at offset 200
        while data.len() < 200 {
            data.push(b' ');
        }
        let xref2_offset = data.len();
        data.extend_from_slice(b"xref\n0 1\n");
        data.extend_from_slice(format_xref_entry(0, 65535, false).as_bytes());
        // Trailer 2 points to Prev = 100 (xref1)
        data.extend_from_slice(format!("trailer\n<< /Prev {} >>\n", xref1_offset).as_bytes());

        // Add end of file markers just in case
        data.extend_from_slice(b"startxref\n0\n%%EOF");

        let mut parser = PdfParser::from(data.as_slice());
        let result = merge_xref_chain(&mut parser, xref2_offset);

        // It should succeed by breaking the loop, not crash or hang.
        assert!(
            result.is_ok(),
            "Failed circular xref test: {:?}",
            result.err()
        );
        // We expect it to visit xref2, then xref1, then see xref2 again and stop.
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

        let mut reader = PdfReader;
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

        let mut reader = PdfReader;
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

        let mut reader = PdfReader;
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
}
