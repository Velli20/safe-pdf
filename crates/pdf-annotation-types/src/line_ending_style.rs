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
    Unknown(Vec<u8>),
}

impl From<&[u8]> for LineEndingStyle {
    fn from(value: &[u8]) -> Self {
        match value {
            b"S" => Self::Slash,
            b"B" | b"Square" => Self::Square,
            b"C" | b"Circle" => Self::Circle,
            b"D" | b"Diamond" => Self::Diamond,
            b"OpenArrow" => Self::OpenArrow,
            b"ClosedArrow" => Self::ClosedArrow,
            b"Butt" => Self::Butt,
            b"None" => Self::None,
            other => Self::Unknown(Vec::from(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LineEndingStyle;

    #[test]
    fn parses_standard_line_ending_names() {
        assert_eq!(
            LineEndingStyle::from(b"Square".as_slice()),
            LineEndingStyle::Square
        );
        assert_eq!(
            LineEndingStyle::from(b"Circle".as_slice()),
            LineEndingStyle::Circle
        );
        assert_eq!(
            LineEndingStyle::from(b"Diamond".as_slice()),
            LineEndingStyle::Diamond
        );
    }

    #[test]
    fn preserves_short_aliases() {
        assert_eq!(
            LineEndingStyle::from(b"B".as_slice()),
            LineEndingStyle::Square
        );
        assert_eq!(
            LineEndingStyle::from(b"C".as_slice()),
            LineEndingStyle::Circle
        );
        assert_eq!(
            LineEndingStyle::from(b"D".as_slice()),
            LineEndingStyle::Diamond
        );
    }
}
