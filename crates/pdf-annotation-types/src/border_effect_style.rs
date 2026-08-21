/// A border effect style.
pub enum BorderEffectStyle {
    /// No border effect.
    None,
    /// A cloudy border effect.
    Cloudy,
    /// A vendor or future border effect.
    Unknown(Vec<u8>),
}

impl From<&[u8]> for BorderEffectStyle {
    fn from(value: &[u8]) -> Self {
        match value {
            b"S" => Self::None,
            b"C" => Self::Cloudy,
            other => Self::Unknown(Vec::from(other)),
        }
    }
}
