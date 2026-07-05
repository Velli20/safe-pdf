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
    Unknown(String),
}

impl From<&str> for LinkHighlightMode {
    fn from(value: &str) -> Self {
        match value {
            "I" => Self::Invert,
            "O" => Self::Outline,
            "P" => Self::Push,
            "N" => Self::None,
            "T" => Self::Toggle,
            other => Self::Unknown(other.to_owned()),
        }
    }
}
