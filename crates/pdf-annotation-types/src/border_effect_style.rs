/// A border effect style.
pub enum BorderEffectStyle {
    /// No border effect.
    None,
    /// A cloudy border effect.
    Cloudy,
    /// A vendor or future border effect.
    Unknown(String),
}

impl From<&str> for BorderEffectStyle {
    fn from(value: &str) -> Self {
        match value {
            "S" => Self::None,
            "C" => Self::Cloudy,
            other => Self::Unknown(other.to_owned()),
        }
    }
}
