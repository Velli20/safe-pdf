/// Represents the blend mode used for compositing graphics.
///
/// Blend modes determine how colors from different layers are combined.
#[derive(Debug, PartialEq, Clone)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    DestinationIn,
    Unknown(Vec<u8>),
}

impl From<&[u8]> for BlendMode {
    fn from(value: &[u8]) -> Self {
        match value {
            b"Normal" => Self::Normal,
            b"Multiply" => Self::Multiply,
            b"Screen" => Self::Screen,
            b"Overlay" => Self::Overlay,
            b"Darken" => Self::Darken,
            b"Lighten" => Self::Lighten,
            b"ColorDodge" => Self::ColorDodge,
            b"ColorBurn" => Self::ColorBurn,
            b"HardLight" => Self::HardLight,
            b"SoftLight" => Self::SoftLight,
            b"Difference" => Self::Difference,
            b"Exclusion" => Self::Exclusion,
            b"Hue" => Self::Hue,
            b"Saturation" => Self::Saturation,
            b"Color" => Self::Color,
            b"Luminosity" => Self::Luminosity,
            other => Self::Unknown(Vec::from(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BlendMode;

    #[test]
    fn preserves_unknown_blend_mode() {
        assert_eq!(
            BlendMode::from(b"VendorBlend".as_slice()),
            BlendMode::Unknown(Vec::from(b"VendorBlend"))
        );
    }
}
