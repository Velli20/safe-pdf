use std::collections::BTreeMap;

use crate::trailer::Trailer;

/// Represents a cross-reference table in a PDF file.
/// The cross-reference table is used to quickly locate objects in the PDF file
/// without having to read the entire file. It is typically found at the end of
/// the PDF file, and it is preceded by a trailer dictionary that contains
/// information about the file, such as the number of objects and the size of
/// the file.
#[derive(Debug, PartialEq, Clone)]
pub struct CrossReferenceTable {
    /// The map of object numbers to cross-reference entries.
    pub entries: BTreeMap<usize, CrossReferenceEntryType>,
    /// The trailer associated with this cross-reference table.
    pub trailer: Trailer,
}

impl CrossReferenceTable {
    pub fn new(entries: BTreeMap<usize, CrossReferenceEntryType>, trailer: Trailer) -> Self {
        CrossReferenceTable { entries, trailer }
    }
}

/// Represents a cross-reference entry.
#[derive(Debug, PartialEq, Clone)]
pub enum CrossReferenceEntryType {
    /// Type 1: object at a byte offset in the file (traditional).
    Normal {
        byte_offset: usize,
        generation_number: usize,
    },
    /// Type 2: object stored in a compressed object stream.
    Compressed {
        object_stream_number: usize,
        index_within_stream: usize,
    },
    /// Type 0: free object (not in use).
    Free {
        next_free_object: usize,
        generation_number: usize,
    },
}

impl CrossReferenceEntryType {
    /// Creates a normal entry for an uncompressed object at a byte offset.
    pub fn new_normal(byte_offset: usize, generation_number: usize) -> Self {
        CrossReferenceEntryType::Normal {
            byte_offset,
            generation_number,
        }
    }

    /// Creates an entry for an object stored within a compressed object stream.
    pub fn new_compressed(object_stream_number: usize, index_within_stream: usize) -> Self {
        CrossReferenceEntryType::Compressed {
            object_stream_number,
            index_within_stream,
        }
    }

    /// Creates an entry for a free object.
    pub fn new_free(next_free_object: usize, generation_number: usize) -> Self {
        CrossReferenceEntryType::Free {
            next_free_object,
            generation_number,
        }
    }

    /// Returns the byte offset if this is a Normal entry.
    pub fn byte_offset(&self) -> Option<usize> {
        match self {
            CrossReferenceEntryType::Normal { byte_offset, .. } => Some(*byte_offset),
            _ => None,
        }
    }

    /// Returns true if this is a Normal (in-use, uncompressed) entry.
    pub fn is_normal(&self) -> bool {
        matches!(self, CrossReferenceEntryType::Normal { .. })
    }

    /// Returns true if this is a Free entry.
    pub fn is_free(&self) -> bool {
        matches!(self, CrossReferenceEntryType::Free { .. })
    }

    /// Returns true if this is a Compressed entry.
    pub fn is_compressed(&self) -> bool {
        matches!(self, CrossReferenceEntryType::Compressed { .. })
    }
}

/// Represents the status of a cross-reference entry in a PDF file.
/// The status indicates whether the object is normal, free, or old.
/// Used by the traditional xref table parser.
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
