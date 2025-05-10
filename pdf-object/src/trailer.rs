use std::rc::Rc;

use crate::dictionary::Dictionary;

/// Represents the trailer of a PDF document.
///
/// It contains a dictionary with global information about the document, such as
/// a reference to the document catalog (`/Root`) and the total number of
/// objects (`/Size`).
#[derive(Debug, PartialEq, Clone)]
pub struct Trailer {
    /// The dictionary object containing the trailer information.
    pub dictionary: Rc<Dictionary>,
    /// The byte offset from the beginning of the file to the start of
    /// the cross-reference table (`xref` section), used for locating
    /// objects within the PDF.
    pub offset: u32,
}

impl Trailer {
    pub fn new(dictionary: Rc<Dictionary>, offset: u32) -> Self {
        Trailer { dictionary, offset }
    }
}
