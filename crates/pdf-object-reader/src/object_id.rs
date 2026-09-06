//! Indirect PDF object identifiers.

/// Identifies one indirect object in a PDF file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId {
    /// The indirect object number.
    pub number: usize,
    /// The indirect generation number.
    pub generation: usize,
}

impl ObjectId {
    /// Creates an indirect object identifier.
    pub fn new(number: usize, generation: usize) -> Self {
        Self { number, generation }
    }

    /// Returns the indirect object number.
    pub fn number(self) -> usize {
        self.number
    }

    /// Returns the generation number associated with the object.
    pub fn generation(self) -> usize {
        self.generation
    }
}
