/// A caret symbol style.
pub enum CaretSymbolStyle {
    /// Insert text marker.
    P,
    /// No marker.
    None,
    /// A vendor or future caret symbol style.
    Unknown(Vec<u8>),
}

impl From<&[u8]> for CaretSymbolStyle {
    fn from(value: &[u8]) -> Self {
        match value {
            b"P" => Self::P,
            b"None" => Self::None,
            other => Self::Unknown(Vec::from(other)),
        }
    }
}
