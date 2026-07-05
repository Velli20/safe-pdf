/// A caret symbol style.
pub enum CaretSymbolStyle {
    /// Insert text marker.
    P,
    /// No marker.
    None,
    /// A vendor or future caret symbol style.
    Unknown(String),
}

impl From<&str> for CaretSymbolStyle {
    fn from(value: &str) -> Self {
        match value {
            "P" => Self::P,
            "None" => Self::None,
            other => Self::Unknown(other.to_owned()),
        }
    }
}
