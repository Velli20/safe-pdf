/// Specifies the mode for applying a soft mask in PDF graphics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskMode {
    /// The soft mask is applied to the alpha channel only.
    Alpha,
    /// The soft mask is applied to the luminosity channel.
    Luminosity,
    /// An unrecognized soft mask mode.
    Unknown(Vec<u8>),
}

impl From<&[u8]> for MaskMode {
    fn from(value: &[u8]) -> Self {
        match value {
            b"Alpha" => Self::Alpha,
            b"Luminosity" => Self::Luminosity,
            other => Self::Unknown(Vec::from(other)),
        }
    }
}
