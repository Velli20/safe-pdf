/// A line ending style.
#[derive(Debug, Clone, PartialEq, Eq)]
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
            "B" | "Square" => Self::Square,
            "C" | "Circle" => Self::Circle,
            "D" | "Diamond" => Self::Diamond,
            "OpenArrow" => Self::OpenArrow,
            "ClosedArrow" => Self::ClosedArrow,
            "Butt" => Self::Butt,
            "None" => Self::None,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LineEndingStyle;

    #[test]
    fn parses_standard_line_ending_names() {
        assert_eq!(LineEndingStyle::from("Square"), LineEndingStyle::Square);
        assert_eq!(LineEndingStyle::from("Circle"), LineEndingStyle::Circle);
        assert_eq!(LineEndingStyle::from("Diamond"), LineEndingStyle::Diamond);
    }

    #[test]
    fn preserves_short_aliases() {
        assert_eq!(LineEndingStyle::from("B"), LineEndingStyle::Square);
        assert_eq!(LineEndingStyle::from("C"), LineEndingStyle::Circle);
        assert_eq!(LineEndingStyle::from("D"), LineEndingStyle::Diamond);
    }
}
