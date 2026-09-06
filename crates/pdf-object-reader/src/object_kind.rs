//! Runtime classifications of PDF objects.

/// Classifies the runtime kind of a PDF object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    /// The PDF null value.
    Null,
    /// A PDF boolean.
    Boolean,
    /// A PDF integer.
    Integer,
    /// A PDF real number.
    Real,
    /// A PDF byte string.
    String,
    /// A PDF name.
    Name,
    /// A PDF array.
    Array,
    /// A PDF dictionary.
    Dictionary,
    /// A PDF stream.
    Stream,
    /// An indirect object reference.
    Reference,
    /// A parser trailer value.
    Trailer,
    /// A parsed cross-reference table.
    CrossReferenceTable,
    /// The parser end-of-file marker.
    EndOfFile,
}
