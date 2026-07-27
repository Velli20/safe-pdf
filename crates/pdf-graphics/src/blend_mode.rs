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
    Unknown(String),
}

impl From<&str> for BlendMode {
    fn from(value: &str) -> Self {
        match value {
            "Normal" => Self::Normal,
            "Multiply" => Self::Multiply,
            "Screen" => Self::Screen,
            "Overlay" => Self::Overlay,
            "Darken" => Self::Darken,
            "Lighten" => Self::Lighten,
            "ColorDodge" => Self::ColorDodge,
            "ColorBurn" => Self::ColorBurn,
            "HardLight" => Self::HardLight,
            "SoftLight" => Self::SoftLight,
            "Difference" => Self::Difference,
            "Exclusion" => Self::Exclusion,
            "Hue" => Self::Hue,
            "Saturation" => Self::Saturation,
            "Color" => Self::Color,
            "Luminosity" => Self::Luminosity,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BlendMode;

    #[test]
    fn preserves_unknown_blend_mode() {
        assert_eq!(
            BlendMode::from("VendorBlend"),
            BlendMode::Unknown("VendorBlend".to_owned())
        );
    }
}
