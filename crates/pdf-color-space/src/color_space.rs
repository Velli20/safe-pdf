use pdf_graphics::color::Color;

use crate::{
    cal_gray_color_space::CalGrayColorSpace, cal_rgb_color_space::CalRGBColorSpace,
    device_n_color_space::DeviceNColorSpace, error::ColorSpaceError,
    icc_based_color_space::ICCBasedColorSpace, indexed_color_space::IndexedColorSpace,
    lab_color_space::LabColorSpace, separation_color_space::SeparationColorSpace,
};

#[derive(Debug, Clone)]
pub enum ColorSpace {
    /// Grayscale color space with a single component (0.0 = black, 1.0 = white).
    DeviceGray,
    /// RGB color space with three components (Red, Green, Blue).
    DeviceRGB,
    /// CMYK color space with four components (Cyan, Magenta, Yellow, Black).
    DeviceCMYK,
    /// Indexed (palette-based) color space.
    Indexed(IndexedColorSpace),
    /// ICC profile-based color space.
    ICCBased(ICCBasedColorSpace),
    /// Separation color space (single spot colorant).
    Separation(SeparationColorSpace),
    /// CIE 1976 L*a*b* color space.
    Lab(LabColorSpace),
    /// Calibrated Gray color space.
    CalGray(CalGrayColorSpace),
    /// Calibrated RGB color space.
    CalRGB(CalRGBColorSpace),
    /// DeviceN multi-colorant color space.
    DeviceN(DeviceNColorSpace),
    /// Pattern color space. Painting uses a pattern resource, not
    /// plain color components. The optional inner space is the underlying color
    /// space for uncolored tiling patterns (e.g. `[/Pattern /DeviceGray]`).
    Pattern(Option<Box<ColorSpace>>),
}

impl ColorSpace {
    /// Returns the number of color components required as *input* to this color space.
    ///
    /// For `Indexed`, this is always 1 (the palette index), regardless of how many
    /// components the base space uses.
    #[must_use]
    pub fn num_color_components(&self) -> usize {
        match self {
            Self::DeviceGray | Self::CalGray(_) => 1,
            Self::DeviceRGB | Self::CalRGB(_) | Self::Lab(_) => 3,
            Self::DeviceCMYK => 4,
            // Indexed takes a single integer index as input.
            Self::Indexed(_) => 1,
            Self::ICCBased(icc) => icc.num_components,
            Self::Separation(_) => 1,
            Self::DeviceN(dn) => dn.num_components(),
            // Pattern color is set by the pattern resource, not by numeric components.
            Self::Pattern(_) => 0,
        }
    }

    /// Returns the number of bits per pixel given the bits per component.
    ///
    /// Calculated as `bits_per_component * num_color_components()`.
    /// Uses saturating multiplication to prevent overflow.
    #[must_use]
    pub fn bits_per_pixel(&self, bits_per_component: usize) -> usize {
        bits_per_component.saturating_mul(self.num_color_components())
    }

    /// Returns `true` if this is a device-dependent color space.
    #[must_use]
    pub const fn is_device_space(&self) -> bool {
        matches!(self, Self::DeviceGray | Self::DeviceRGB | Self::DeviceCMYK)
    }

    /// Returns the device space that draws this space's samples unchanged, if there is one.
    ///
    /// A device space is its own equivalent. An ICCBased space draws through its
    /// alternate until ICC colour management exists, so it has one when that alternate
    /// does.
    #[must_use]
    pub fn device_equivalent(&self) -> Option<Self> {
        match self {
            Self::ICCBased(icc) => icc.device_equivalent(),
            space if space.is_device_space() => Some(space.clone()),
            _ => None,
        }
    }

    pub fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        match self {
            ColorSpace::Indexed(indexed) => indexed.apply(components),
            ColorSpace::Separation(separation) => separation.apply(components),
            ColorSpace::ICCBased(icc) => icc.apply(components),
            ColorSpace::Lab(lab) => lab.apply(components),
            ColorSpace::CalGray(cal_gray) => cal_gray.apply(components),
            ColorSpace::CalRGB(cal_rgb) => cal_rgb.apply(components),
            ColorSpace::DeviceN(dn) => dn.apply(components),
            device_space @ (ColorSpace::DeviceGray
            | ColorSpace::DeviceRGB
            | ColorSpace::DeviceCMYK) => {
                let expected_components = device_space.num_color_components();
                Color::from_device_components(components)
                    .filter(|_| components.len() == expected_components)
                    .ok_or(ColorSpaceError::InsufficientComponents(
                        expected_components,
                        components.len(),
                    ))
            }
            ColorSpace::Pattern(_) => Err(ColorSpaceError::Unsupported(
                "Pattern color space: color is set by the pattern resource, not by components"
                    .into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use pdf_graphics::color::Color;

    use super::ColorSpace;
    use crate::error::ColorSpaceError;

    #[test]
    fn applies_device_color_space_components() {
        assert_eq!(
            ColorSpace::DeviceGray.apply(&[0.5]).expect("valid gray"),
            Color::from_gray(0.5)
        );
        assert_eq!(
            ColorSpace::DeviceRGB
                .apply(&[0.1, 0.2, 0.3])
                .expect("valid RGB"),
            Color::from_rgb(0.1, 0.2, 0.3)
        );
        assert_eq!(
            ColorSpace::DeviceCMYK
                .apply(&[0.1, 0.2, 0.3, 0.4])
                .expect("valid CMYK"),
            Color::from_cmyk(0.1, 0.2, 0.3, 0.4)
        );
    }

    #[test]
    fn device_color_space_remains_strict_about_component_count() {
        let error = ColorSpace::DeviceRGB
            .apply(&[0.5])
            .expect_err("DeviceRGB must not interpret one component as gray");

        assert!(matches!(
            error,
            ColorSpaceError::InsufficientComponents(3, 1)
        ));
    }
}
