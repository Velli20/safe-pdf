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
    Unknown(Vec<u8>),
}

impl From<&[u8]> for BorderStyleName {
    fn from(value: &[u8]) -> Self {
        match value {
            b"S" => Self::Solid,
            b"D" => Self::Dashed,
            b"B" => Self::Beveled,
            b"I" => Self::Inset,
            b"U" => Self::Underline,
            other => Self::Unknown(Vec::from(other)),
        }
    }
}
