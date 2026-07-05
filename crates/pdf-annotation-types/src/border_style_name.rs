/// A border style name.
pub enum BorderStyleName {
    /// Solid line.
    Solid,
    /// Dashed line.
    Dashed,
    /// Beveled border.
    Beveled,
    /// Inset border.
    Inset,
    /// Underline border.
    Underline,
    /// A vendor or future border style.
    Unknown(String),
}

impl From<&str> for BorderStyleName {
    fn from(value: &str) -> Self {
        match value {
            "S" => Self::Solid,
            "D" => Self::Dashed,
            "B" => Self::Beveled,
            "I" => Self::Inset,
            "U" => Self::Underline,
            other => Self::Unknown(other.to_owned()),
        }
    }
}
