/// A line ending style.
#[derive(Debug, Clone, PartialEq)]
pub enum LineEndingStyle {
    /// A butt end.
    Butt,
    /// A circular end.
    Circle,
    /// A diamond end.
    Diamond,
    /// An open arrow end.
    OpenArrow,
    /// A closed arrow end.
    ClosedArrow,
    /// No end marker.
    None,
    /// A square end.
    Square,
    /// A slashed end.
    Slash,
    /// A vendor or future line ending style.
    Unknown(String),
}

impl From<&str> for LineEndingStyle {
    fn from(value: &str) -> Self {
        match value {
            "S" => Self::Slash,
            "B" => Self::Square,
            "C" => Self::Circle,
            "D" => Self::Diamond,
            "OpenArrow" => Self::OpenArrow,
            "ClosedArrow" => Self::ClosedArrow,
            "Butt" => Self::Butt,
            "None" => Self::None,
            other => Self::Unknown(other.to_owned()),
        }
    }
}
