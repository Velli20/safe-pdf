use std::collections::{BTreeMap, HashSet};

use crate::decryption::{DecryptionError, DocumentDecryptor};
use crate::document::PdfDocument;
use pdf_object::{
    ObjectVariant,
    cross_reference_table::{CrossReferenceEntry, CrossReferenceStatus, CrossReferenceTable},
    error::ObjectError,
    object_collection::ObjectCollection,
    stream::StreamObject,
    trailer::Trailer,
    traits::FromDictionary,
};
use pdf_page::pages::{PdfPages, PdfPagesError};
use pdf_parser::{
    error::ParserError, header::HeaderError, parser::PdfParser, traits::HeaderParser,
};
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
    /// # Arguments
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
        let CrossReferenceTable { entries, trailer } = build_xref_index(&mut parser)?;

        // Check for encryption and handle it before loading other objects.
        let decryptor = if let Some(encrypt_ref) = trailer.dictionary.get("Encrypt") {
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
        let objects = load_objects_with_decryption(&entries, &mut parser, decryptor.as_ref())?;

        // Extract catalog and page tree
        let pages = extract_page_tree(&trailer, &objects)?;

        Ok(PdfDocument {
            objects,
            pages: pages.pages,
        })
    }
}

/// Preloads the cross-reference (xref) table for classic (table-based) PDFs.
///
/// This method builds a complete xref index by:
/// 1. Locating the final `trailer` keyword at the end of the file
/// 2. Following the chain of cross-reference tables via `/Prev` entries
/// 3. Merging xref entries (newer entries take precedence)
/// 4. Selecting the best trailer (one with `/Root` if available)
///
/// # Returns
///
/// Returns `CrossReferenceTable` on success or a `PdfReaderError` if the xref structure is invalid.
fn build_xref_index(parser: &mut PdfParser) -> Result<CrossReferenceTable, PdfReaderError> {
    // Locate the final "trailer" keyword by scanning backwards from the end
    const TRAILER_KEYWORD: &[u8] = b"trailer";
    let trailer_pos = parser
        .tokenizer
        .input
        .windows(TRAILER_KEYWORD.len())
        .rposition(|window| window == TRAILER_KEYWORD)
        .ok_or(PdfReaderError::MissingTrailer)?;

    // Parse the trailer to get the startxref offset
    let ObjectVariant::Trailer(initial_trailer) = parser.parse_object_at(trailer_pos, None)? else {
        return Err(PdfReaderError::MissingTrailer);
    };

    // Follow the xref chain, merging entries from all linked tables
    merge_xref_chain(parser, initial_trailer.offset)
}

/// Follows the xref chain via `/Prev` entries and merges all cross-reference tables.
///
/// This handles incremental PDF updates where each update adds a new xref section
/// that references the previous one via the `/Prev` entry in the trailer.
///
/// # Returns
///
/// Returns `CrossReferenceTable` on success or a `PdfReaderError` if the xref structure is invalid.
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

        // Parse the xref table at the current offset
        let ObjectVariant::CrossReferenceTable(xref_table) =
            parser.parse_object_at(current_offset, None)?
        else {
            return Err(PdfReaderError::InvalidXrefAtOffset {
                offset: current_offset,
            });
        };

        // Merge entries: newer entries (already in merged_xref) take precedence
        for (obj_num, entry) in xref_table.entries {
            // Only insert if the object number doesn't already exist
            entries.entry(obj_num).or_insert(entry);
        }

        let prev_value = xref_table.trailer.dictionary.get("Prev").cloned();

        // Select the best trailer: prefer one with a `/Root` entry
        match trailer.as_ref() {
            None => {
                // First trailer becomes the initial candidate
                trailer = Some(xref_table.trailer);
            }
            Some(existing) if existing.dictionary.get("Root").is_none() => {
                // Replace if current trailer has a `/Root` entry
                if xref_table.trailer.dictionary.get("Root").is_some() {
                    trailer = Some(xref_table.trailer);
                }
            }
            _ => {}
        }

        // Follow the chain to the previous xref section
        if let Some(prev_value) = prev_value {
            current_offset = prev_value.as_number::<usize>()?;
        } else {
            // No more previous sections
            break;
        }
    }

    let trailer = trailer.ok_or(PdfReaderError::MissingTrailer)?;

    Ok(CrossReferenceTable::new(entries, trailer))
}

/// Extracts the page tree from the document catalog.
///
/// Follows the chain: Trailer → /Root (Catalog) → /Pages (Page Tree)
///
/// # Returns
///
/// Returns a `PdfPages` structure containing the document's page hierarchy.
fn extract_page_tree(
    trailer: &Trailer,
    objects: &ObjectCollection,
) -> Result<PdfPages, PdfReaderError> {
    // Get the document catalog via the /Root entry in the trailer
    let root_ref = trailer.dictionary.get_or_err("Root")?;
    let catalog = objects.resolve_dictionary(root_ref)?;

    // Get the page tree via the /Pages entry in the catalog
    let pages_ref = catalog.get_or_err("Pages")?;
    let pages_dict = objects.resolve_dictionary(pages_ref)?;

    // Parse the page tree structure
    PdfPages::from_dictionary(pages_dict, objects).map_err(Into::into)
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
/// # Arguments
///
/// - `encrypt_ref`: The `/Encrypt` entry from the trailer (usually an indirect reference).
/// - `entries`: The cross-reference table entries for locating objects.
/// - `parser`: The PDF parser for reading object data.
///
/// # Returns
///
/// Returns an `EncryptDictionary` containing the encryption parameters.
fn load_encrypt_dictionary(
    encrypt_ref: &ObjectVariant,
    entries: &BTreeMap<usize, CrossReferenceEntry>,
    parser: &mut PdfParser,
) -> Result<EncryptDictionary, PdfReaderError> {
    // If the Encrypt entry is an indirect reference, we need to load that object.
    let encrypt_dict = match encrypt_ref {
        ObjectVariant::Reference(obj_num) => {
            // Look up the object in the xref table
            let entry = entries
                .get(obj_num)
                .ok_or(ObjectError::FailedResolveObjectReference { obj_num: *obj_num })?;

            if entry.status != CrossReferenceStatus::Normal {
                return Err(ObjectError::FailedResolveObjectReference { obj_num: *obj_num }.into());
            }

            // Parse the encryption object at the specified offset
            let object = parser.parse_object_at(entry.byte_offset, None)?;

            // Extract the dictionary from the parsed object
            match object {
                ObjectVariant::Dictionary(dict) => dict,
                ObjectVariant::IndirectObject(indirect) => match indirect.object.as_ref() {
                    Some(ObjectVariant::Dictionary(dict)) => std::rc::Rc::clone(dict),
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
        ObjectVariant::Dictionary(dict) => std::rc::Rc::clone(dict),
        other => {
            return Err(ObjectError::FailedResolveDictionaryObject {
                resolved_type: other.name(),
            }
            .into());
        }
    };

    // Parse the encryption dictionary
    let objects = ObjectCollection::default();
    EncryptDictionary::from_dictionary(&encrypt_dict, &objects).map_err(Into::into)
}

/// Extracts the document ID from the trailer's /ID array.
///
/// The /ID entry is an array of two byte strings that uniquely identify the document.
/// The first element is used for encryption key derivation.
///
/// # Arguments
///
/// - `trailer`: The PDF trailer containing the /ID entry.
///
/// # Returns
///
/// The first element of the /ID array as a byte vector.
fn extract_document_id(trailer: &Trailer) -> Result<Vec<u8>, PdfReaderError> {
    let id_entry = trailer
        .dictionary
        .get("ID")
        .ok_or(PdfReaderError::MissingDocumentId)?;

    match id_entry {
        ObjectVariant::Array(arr) => {
            if arr.is_empty() {
                return Err(PdfReaderError::MissingDocumentId);
            }
            // Get the first element
            arr[0]
                .try_bytes()
                .map(|b| b.to_vec())
                .map_err(|_| PdfReaderError::MissingDocumentId)
        }
        _ => Err(PdfReaderError::MissingDocumentId),
    }
}

/// Loads all objects referenced in the cross-reference table with optional decryption.
///
/// This function extends `load_objects` by decrypting stream data when a decryptor
/// is provided. Only streams are decrypted; strings within dictionaries are decrypted
/// separately during object resolution.
///
/// # Arguments
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

    for entry in entries.values().rev() {
        // Only load normal objects.
        if entry.status != CrossReferenceStatus::Normal {
            continue;
        }

        // Parse the object at the specified byte offset
        let object = parser.parse_object_at(entry.byte_offset, Some(&objects))?;

        // Sanity check: objects at xref entries shouldn't be bare references
        if matches!(object, ObjectVariant::Reference(_)) {
            return Err(PdfReaderError::UnexpectedReference {
                offset: entry.byte_offset,
            });
        }

        // Decrypt stream data if we have a decryptor
        let object = if let Some(decryptor) = decryptor {
            decrypt_object(object, decryptor)?
        } else {
            object
        };

        objects.insert(object)?;
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
            if let Some(ObjectVariant::Stream(stream)) = &indirect.object {
                let decrypted_stream = decrypt_stream_object(stream, decryptor)?;
                // Create a new IndirectObject with the decrypted stream
                let new_indirect = pdf_object::indirect_object::IndirectObject::new(
                    indirect.object_number,
                    indirect.generation_number,
                    Some(ObjectVariant::Stream(std::rc::Rc::new(decrypted_stream))),
                );
                return Ok(ObjectVariant::IndirectObject(std::rc::Rc::new(
                    new_indirect,
                )));
            }
            // Non-stream indirect objects pass through unchanged
            Ok(ObjectVariant::IndirectObject(indirect))
        }
        ObjectVariant::Stream(stream) => {
            let decrypted = decrypt_stream_object(&stream, decryptor)?;
            Ok(ObjectVariant::Stream(std::rc::Rc::new(decrypted)))
        }
        // Other objects pass through unchanged
        // (string decryption would be handled separately during resolution)
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
        std::rc::Rc::clone(&stream.dictionary),
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
        assert_eq!(entry1.byte_offset, obj1_offset);

        // Check free entry
        let entry0 = table.entries.get(&0).expect("Obj 0 should exist");
        assert!(
            format!("{:?}", entry0.status)
                .to_lowercase()
                .contains("free"),
            "Obj 0 should be free"
        );

        // Check trailer
        let size: i64 = table
            .trailer
            .dictionary
            .get("Size")
            .expect("Size expected")
            .as_number()
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
            entry1.byte_offset, obj1_v2_offset,
            "Obj 1 should point to v2"
        );

        // Check Obj 2 (should be from v1)
        let entry2 = table.entries.get(&2).expect("Obj 2 missing");
        assert_eq!(entry2.byte_offset, obj2_offset, "Obj 2 should be from v1");
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

        let mut reader = PdfReader::default();
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

        let mut reader = PdfReader::default();
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

        let mut reader = PdfReader::default();
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
