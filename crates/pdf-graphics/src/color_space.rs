use std::borrow::Cow;

/// Represents the color space of an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    /// The DeviceRGB color space.
    DeviceRGB,
    /// The DeviceCMYK color space.
    DeviceCMYK,
    /// The DeviceGray color space.
    DeviceGray,
    /// The Indexed color space.
    Indexed,
    /// The ICCBased color space.
    ICCBased,
}

impl From<Cow<'_, str>> for ColorSpace {
    fn from(value: Cow<'_, str>) -> Self {
        match value.as_ref() {
            "DeviceRGB" => ColorSpace::DeviceRGB,
            "DeviceCMYK" => ColorSpace::DeviceCMYK,
            "DeviceGray" => ColorSpace::DeviceGray,
            "Indexed" => ColorSpace::Indexed,
            "ICCBased" => ColorSpace::ICCBased,
            _ => ColorSpace::DeviceRGB,
        }
    }
}

impl ColorSpace {
    /// Returns the number of color components for the color space.
    pub const fn num_color_components(&self) -> usize {
        match self {
            ColorSpace::DeviceGray => 1,
            ColorSpace::DeviceRGB => 3,
            ColorSpace::DeviceCMYK => 4,
            ColorSpace::Indexed => 1,
            ColorSpace::ICCBased => 0,
        }
    }

    /// Returns the number of bits per pixel for the color space given bits per component.
    pub const fn bits_per_pixel(&self, bits_per_component: usize) -> usize {
        bits_per_component.saturating_mul(self.num_color_components())
    }
}
