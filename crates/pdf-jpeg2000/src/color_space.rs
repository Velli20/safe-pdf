//! Enumerated colour spaces named by a JP2 Colour Specification box.
//!
//! Part 1 Annex I defines the enumeration for JP2 and Part 2 Annex M extends
//! it for JPX. The decoder records which space a file names and how many
//! colour channels it implies; it performs no colour transform, because a PDF
//! `/ColorSpace` entry may override the embedded description entirely.

use crate::container::ColorSpecification;

/// A colour space named by the `EnumCS` field of a Colour Specification box.
///
/// Unknown values are rejected rather than mapped to a nearby space, so a
/// caller can distinguish a space this decoder does not model from one it
/// models but cannot convert.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EnumeratedColorSpace {
    /// One-bit bilevel image where zero is white.
    BilevelWhiteZero,
    /// ITU-R BT.601 luma and chroma.
    YCbCr601,
    /// ITU-R BT.709 luma and chroma.
    YCbCr709,
    /// SMPTE 240M luma and chroma.
    YCbCr240M,
    /// Kodak PhotoYCC.
    PhotoYcc,
    /// Subtractive cyan, magenta, and yellow.
    Cmy,
    /// Subtractive cyan, magenta, yellow, and black, required by PDF for JPX.
    Cmyk,
    /// Luma with subtractive chroma and black.
    Ycck,
    /// CIE 1976 L*a*b*.
    CieLab,
    /// One-bit bilevel image where zero is black.
    BilevelBlackZero,
    /// IEC 61966-2-1 sRGB.
    Srgb,
    /// Single-channel greyscale.
    Greyscale,
    /// Luma and chroma derived from sRGB.
    Sycc,
    /// CIE 1976 J*a*b*.
    CieJab,
    /// IEC 61966-2-2 extended-range sRGB.
    ExtendedSrgb,
    /// ISO 22028-2 ROMM RGB.
    RommRgb,
    /// SMPTE 240M component video.
    YPbPr1125,
    /// ITU-R BT.601 component video.
    YPbPr1250,
    /// Extended-range sYCC.
    ExtendedSycc,
}

impl EnumeratedColorSpace {
    /// Returns the number of colour channels the space defines.
    ///
    /// Opacity channels are described by the Channel Definition box instead,
    /// so this count never includes them.
    pub fn color_channels(self) -> u16 {
        match self {
            Self::BilevelWhiteZero | Self::BilevelBlackZero | Self::Greyscale => 1,
            Self::Cmyk | Self::Ycck => 4,
            _ => 3,
        }
    }

    /// Returns whether a PDF renderer can convert this space without a profile.
    ///
    /// ISO 32000 expects a JPX image to arrive in one of these spaces when the
    /// PDF image dictionary supplies no `/ColorSpace` of its own.
    pub fn is_pdf_renderable(self) -> bool {
        matches!(
            self,
            Self::Greyscale
                | Self::Srgb
                | Self::Sycc
                | Self::ExtendedSrgb
                | Self::ExtendedSycc
                | Self::Cmyk
                | Self::Cmy
                | Self::BilevelWhiteZero
                | Self::BilevelBlackZero
        )
    }
}

impl TryFrom<u32> for EnumeratedColorSpace {
    type Error = u32;

    /// Maps an `EnumCS` value, returning the unrecognised code on failure.
    fn try_from(code: u32) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::BilevelWhiteZero),
            1 => Ok(Self::YCbCr601),
            3 => Ok(Self::YCbCr709),
            4 => Ok(Self::YCbCr240M),
            9 => Ok(Self::PhotoYcc),
            11 => Ok(Self::Cmy),
            12 => Ok(Self::Cmyk),
            13 => Ok(Self::Ycck),
            14 => Ok(Self::CieLab),
            15 => Ok(Self::BilevelBlackZero),
            16 => Ok(Self::Srgb),
            17 => Ok(Self::Greyscale),
            18 => Ok(Self::Sycc),
            19 => Ok(Self::CieJab),
            20 => Ok(Self::ExtendedSrgb),
            21 => Ok(Self::RommRgb),
            22 => Ok(Self::YPbPr1125),
            23 => Ok(Self::YPbPr1250),
            24 => Ok(Self::ExtendedSycc),
            unknown => Err(unknown),
        }
    }
}

impl ColorSpecification<'_> {
    /// Returns the named colour space when the box enumerates a known one.
    ///
    /// An ICC profile, or an enumeration outside Annex I and Annex M, yields
    /// `None`; the caller then relies on the profile or on the PDF colour
    /// space that overrides it.
    pub fn enumerated_space(&self) -> Option<EnumeratedColorSpace> {
        match *self {
            Self::Enumerated { code } => EnumeratedColorSpace::try_from(code).ok(),
            Self::Icc { .. } => None,
        }
    }
}
