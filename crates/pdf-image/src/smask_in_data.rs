//! The `/SMaskInData` entry of a JPX image XObject.
//!
//! A JPEG 2000 codestream may carry its own opacity channel. This entry says
//! whether a PDF renderer uses it, and whether the colour samples beside it
//! were already multiplied by it. It applies to no other image filter, and a
//! non-zero value takes the place of a separate `/SMask` image.

use crate::error::PdfImageError;

/// How a JPX image's own opacity channel is used.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum SMaskInData {
    /// Any opacity channel in the image data is discarded.
    #[default]
    Ignore,
    /// The opacity channel supplies alpha for unmultiplied colour samples.
    Opacity,
    /// The opacity channel supplies alpha and colour was multiplied by it.
    PremultipliedOpacity,
}

impl SMaskInData {
    /// Returns whether the image's own opacity replaces a separate `/SMask`.
    pub(crate) fn overrides_soft_mask(self) -> bool {
        !matches!(self, Self::Ignore)
    }
}

impl TryFrom<i64> for SMaskInData {
    type Error = PdfImageError;

    /// Maps the entry's value, rejecting anything ISO 32000 does not define.
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Ignore),
            1 => Ok(Self::Opacity),
            2 => Ok(Self::PremultipliedOpacity),
            value => Err(PdfImageError::UnsupportedSMaskInData { value }),
        }
    }
}
