/// A link highlight mode.
pub enum LinkHighlightMode {
    /// Invert the annotation appearance.
    Invert,
    /// Outline the annotation.
    Outline,
    /// Push the annotation.
    Push,
    /// Toggle no highlight.
    None,
    /// Use the display rectangle.
    Toggle,
    /// A vendor or future mode.
    Unknown(Vec<u8>),
}

impl From<&[u8]> for LinkHighlightMode {
    fn from(value: &[u8]) -> Self {
        match value {
            b"I" => Self::Invert,
            b"O" => Self::Outline,
            b"P" => Self::Push,
            b"N" => Self::None,
            b"T" => Self::Toggle,
            other => Self::Unknown(Vec::from(other)),
        }
    }
}
