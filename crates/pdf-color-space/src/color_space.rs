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

    pub fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        match self {
            ColorSpace::Indexed(indexed) => indexed.apply(components),
            ColorSpace::Separation(separation) => separation.apply(components),
            ColorSpace::ICCBased(icc) => icc.apply(components),
            ColorSpace::Lab(lab) => lab.apply(components),
            ColorSpace::CalGray(cal_gray) => cal_gray.apply(components),
            ColorSpace::CalRGB(cal_rgb) => cal_rgb.apply(components),
            ColorSpace::DeviceN(dn) => dn.apply(components),
            ColorSpace::DeviceGray => match *components {
                [g] => Ok(Color::from_gray(g)),
                _ => Err(ColorSpaceError::InsufficientComponents(1, components.len())),
            },
            ColorSpace::DeviceRGB => match *components {
                [r, g, b] => Ok(Color::from_rgb(r, g, b)),
                _ => Err(ColorSpaceError::InsufficientComponents(3, components.len())),
            },
            ColorSpace::DeviceCMYK => match *components {
                [c, m, y, k] => Ok(Color::from_cmyk(c, m, y, k)),
                _ => Err(ColorSpaceError::InsufficientComponents(4, components.len())),
            },
            ColorSpace::Pattern(_) => Err(ColorSpaceError::Unsupported(
                "Pattern color space: color is set by the pattern resource, not by components"
                    .into(),
            )),
        }
    }
}
