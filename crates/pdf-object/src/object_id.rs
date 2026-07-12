/// Identifies an indirect PDF object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfObjectId {
    /// The indirect object's number.
    pub number: usize,
    /// The indirect object's generation number.
    pub generation: usize,
}
