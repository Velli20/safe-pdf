use std::collections::BTreeMap;

use crate::trailer::Trailer;

/// Represents a cross-reference table in a PDF file.
/// The cross-reference table is used to quickly locate objects in the PDF file
/// without having to read the entire file. It is typically found at the end of
/// the PDF file, and it is preceded by a trailer dictionary that contains
/// information about the file, such as the number of objects and the size of
/// the file.
///
/// The cross-reference table provides the following key functions:
///
/// - Enables quick access to any indirect object in the file
///   by providing its exact byte offset, avoiding the need to parse the entire document.
/// - Maintains information about which objects are in use and which
///   are free, supporting object reuse during incremental updates.
/// - Facilitates appending changes to a PDF file by adding new
///   cross-reference sections and trailers, allowing reconstruction of the document's
///   current state.
///
/// Each entry in the table contains the object number, generation number, and the byte
/// offset of the object in the file. The cross-reference table is typically located at
/// the end of the PDF file, preceded by a trailer dictionary with metadata about the file.
#[derive(Debug, PartialEq, Clone)]
pub struct CrossReferenceTable {
    /// The map of object numbers to cross-reference entries.
    pub entries: BTreeMap<usize, CrossReferenceEntry>,
    /// The trailer associated with this cross-reference table.
    pub trailer: Trailer,
}

impl CrossReferenceTable {
    pub fn new(entries: BTreeMap<usize, CrossReferenceEntry>, trailer: Trailer) -> Self {
        CrossReferenceTable { entries, trailer }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct CrossReferenceEntry {
    /// The byte offset of the object from the beginning of the file.
    /// Padded with leading zeros if necessary. For free objects, this
    /// is the object number of the next free object in a linked list.
    /// For object 0, it's always 0. nnnnnnnnnn (10 digits):
    pub byte_offset: usize,
    /// The generation number of the object. This is a 5-digit number
    /// that is incremented each time the object is modified. It is
    /// used to determine if the object is still valid or if it has
    /// been replaced by a newer version. nnnnn (5 digits):
    pub generation_number: usize,
    /// The status of the object.
    pub status: CrossReferenceStatus,
}

impl CrossReferenceEntry {
    /// Creates a new `CrossReferenceEntry` with the given byte offset,
    /// generation number, and status.
    ///
    /// # Arguments
    ///
    /// * `byte_offset` - The byte offset of the object from the beginning of the file.
    /// * `generation_number` - The generation number of the object.
    /// * `status` - The status of the object.
    ///
    /// # Returns
    ///
    /// A new `CrossReferenceEntry`.
    pub fn new(byte_offset: usize, generation_number: usize, status: CrossReferenceStatus) -> Self {
        CrossReferenceEntry {
            byte_offset,
            generation_number,
            status,
        }
    }
}

/// Represents the status of a cross-reference entry in a PDF file.
/// The status indicates whether the object is normal, free, or old.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CrossReferenceStatus {
    Normal,
    Free,
    Old,
}

impl CrossReferenceStatus {
    pub fn from_byte(c: u8) -> Option<Self> {
        match c {
            b'n' => Some(CrossReferenceStatus::Normal),
            b'f' => Some(CrossReferenceStatus::Free),
            b'o' => Some(CrossReferenceStatus::Old),
            _ => None,
        }
    }
}
