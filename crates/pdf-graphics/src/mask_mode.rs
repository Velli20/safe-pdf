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
