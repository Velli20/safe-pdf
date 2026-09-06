//! PDF string source representations.

/// Records the source syntax used for a PDF string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StringKind {
    /// A string originally written between parentheses.
    Literal,
    /// A string originally written using hexadecimal notation.
    Hexadecimal,
}
