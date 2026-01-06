use std::collections::{BTreeMap, HashSet};

use crate::document::PdfDocument;
use pdf_object::{
    ObjectVariant,
    cross_reference_table::{CrossReferenceEntry, CrossReferenceStatus, CrossReferenceTable},
    error::ObjectError,
    object_collection::ObjectCollection,
    trailer::Trailer,
    traits::FromDictionary,
};
use pdf_page::pages::{PdfPages, PdfPagesError};
use pdf_parser::{
    error::ParserError, header::HeaderError, parser::PdfParser, traits::HeaderParser,
};
use thiserror::Error;

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
}

#[derive(Default)]
pub struct PdfReader {}

impl PdfReader {
    /// Reads and parses a PDF document from raw bytes.
    ///
    /// This method performs the following steps:
    /// 1. Parses the PDF header and validates the version
    /// 2. Builds the cross-reference index to locate all objects
    /// 3. Loads all objects referenced in the xref table
    /// 4. Extracts the document catalog and page tree
    ///
    /// # Arguments
    ///
    /// - `input`: Raw PDF file bytes
    ///
    /// # Returns
    ///
    /// Returns a `PdfDocument` containing the parsed objects and page structure.
    pub fn read_from_bytes(&mut self, input: &[u8]) -> Result<PdfDocument, PdfReaderError> {
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

        // Load all objects from the xref table
        let objects = load_objects(&entries, &mut parser)?;

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

/// Loads all objects referenced in the cross-reference table.
///
/// Only objects with "Normal" status are loaded. Free or compressed objects
/// are skipped as they're handled differently.
///
/// # Returns
///
/// Returns an `ObjectCollection` containing all parsed objects.
fn load_objects(
    entries: &BTreeMap<usize, CrossReferenceEntry>,
    parser: &mut PdfParser,
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

        objects.insert(object)?;
    }

    Ok(objects)
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
}
