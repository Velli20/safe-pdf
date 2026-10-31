//! Owned adaptive templates for generic and refinement bitmap coding.
//!
//! Templates are validated `Copy` values rather than borrowed coordinate bytes.
//! Procedures that have no encoded template, such as halftone grayscale planes
//! and pattern-dictionary collective bitmaps, select a documented constructor
//! instead of synthesizing input bytes that the encoder never wrote.

use crate::{Jbig2Error, Result, cursor::ByteCursor};

/// Largest number of adaptive pixel pairs any generic template carries.
const GENERIC_ADAPTIVE_MAX: usize = 4;
/// Number of adaptive pixel pairs refinement template zero carries.
const REFINEMENT_ADAPTIVE: usize = 2;

/// Generic bitmap context template (T.88 section 6.2.5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericTemplateId {
    /// Sixteen-bit context with four adaptive pixels.
    Zero,
    /// Thirteen-bit context with one adaptive pixel.
    One,
    /// Ten-bit context with one adaptive pixel.
    Two,
    /// Ten-bit context with one adaptive pixel and a taller reference window.
    Three,
}

impl TryFrom<u8> for GenericTemplateId {
    type Error = Jbig2Error;

    fn try_from(code: u8) -> Result<Self> {
        match code {
            0 => Ok(Self::Zero),
            1 => Ok(Self::One),
            2 => Ok(Self::Two),
            3 => Ok(Self::Three),
            _ => Err(Jbig2Error::InvalidResource("generic template identifier")),
        }
    }
}

impl GenericTemplateId {
    /// Number of adaptive pixel pairs this template uses.
    pub fn adaptive_count(self) -> usize {
        match self {
            Self::Zero => GENERIC_ADAPTIVE_MAX,
            Self::One | Self::Two | Self::Three => 1,
        }
    }

    /// Encoded byte length of this template's adaptive coordinates in a header.
    pub fn encoded_len(self) -> usize {
        self.adaptive_count().saturating_mul(2)
    }

    /// Number of arithmetic contexts this template's context values address.
    pub fn context_count(self) -> usize {
        match self {
            Self::Zero => 65_536,
            Self::One | Self::Two | Self::Three => 8192,
        }
    }

    /// Context selecting the typical-prediction decision (T.88 section 6.2.5.7).
    ///
    /// The legacy `pdf-jbig2` decoder reuses template two's value for template
    /// three; T.88 specifies a distinct value, which this decoder uses.
    pub fn typical_prediction_context(self) -> usize {
        match self {
            Self::Zero => 0x9b25,
            Self::One => 0x0795,
            Self::Two => 0x00e5,
            Self::Three => 0x0195,
        }
    }

    /// Default adaptive coordinates from T.88 Table 6.
    fn default_adaptive(self) -> [(i8, i8); GENERIC_ADAPTIVE_MAX] {
        match self {
            Self::Zero => [(3, -1), (-3, -1), (2, -2), (-2, -2)],
            Self::One => [(3, -1), (0, 0), (0, 0), (0, 0)],
            Self::Two | Self::Three => [(2, -1), (0, 0), (0, 0), (0, 0)],
        }
    }
}

/// A generic template together with its resolved adaptive pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenericTemplate {
    id: GenericTemplateId,
    adaptive: [(i8, i8); GENERIC_ADAPTIVE_MAX],
}

impl GenericTemplate {
    /// Read the adaptive coordinates a region or dictionary header carries.
    pub(crate) fn parse(id: GenericTemplateId, cursor: &mut ByteCursor<'_>) -> Result<Self> {
        let mut adaptive = id.default_adaptive();
        for index in 0..id.adaptive_count() {
            let x = cursor.u8()?;
            let y = cursor.u8()?;
            let slot = adaptive
                .get_mut(index)
                .ok_or(Jbig2Error::InvalidResource("generic adaptive pixel"))?;
            *slot = (i8::from_ne_bytes([x]), i8::from_ne_bytes([y]));
        }
        Ok(Self { id, adaptive })
    }

    /// Use the T.88 Table 6 defaults, for procedures that encode no coordinates.
    pub(crate) fn with_defaults(id: GenericTemplateId) -> Self {
        Self {
            id,
            adaptive: id.default_adaptive(),
        }
    }

    /// Template for a pattern dictionary's collective bitmap (T.88 section 6.7.5),
    /// whose first adaptive pixel is one pattern width to the left.
    pub(crate) fn pattern_collective(id: GenericTemplateId, pattern_width: u8) -> Result<Self> {
        let offset = i8::try_from(pattern_width)
            .map_err(|_| Jbig2Error::InvalidResource("pattern width"))?
            .checked_neg()
            .ok_or(Jbig2Error::InvalidResource("pattern width"))?;
        let mut template = Self::with_defaults(id);
        let slot = template
            .adaptive
            .first_mut()
            .ok_or(Jbig2Error::InvalidResource("pattern adaptive pixel"))?;
        *slot = (offset, 0);
        Ok(template)
    }

    /// Return the validated template identifier.
    pub fn id(self) -> GenericTemplateId {
        self.id
    }

    /// Return an adaptive pixel coordinate, or `None` outside the template.
    pub fn pixel(self, index: usize) -> Option<(i8, i8)> {
        if index >= self.id.adaptive_count() {
            return None;
        }
        self.adaptive.get(index).copied()
    }
}

/// Refinement context template (T.88 section 6.3.5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefinementTemplateId {
    /// Thirteen-bit context with two adaptive pixels.
    Zero,
    /// Ten-bit context with no adaptive pixels.
    One,
}

impl TryFrom<u8> for RefinementTemplateId {
    type Error = Jbig2Error;

    fn try_from(code: u8) -> Result<Self> {
        match code {
            0 => Ok(Self::Zero),
            1 => Ok(Self::One),
            _ => Err(Jbig2Error::InvalidResource(
                "refinement template identifier",
            )),
        }
    }
}

impl RefinementTemplateId {
    /// Number of adaptive pixel pairs this template uses.
    pub fn adaptive_count(self) -> usize {
        match self {
            Self::Zero => REFINEMENT_ADAPTIVE,
            Self::One => 0,
        }
    }

    /// Encoded byte length of this template's adaptive coordinates in a header.
    pub fn encoded_len(self) -> usize {
        self.adaptive_count().saturating_mul(2)
    }

    /// Number of arithmetic contexts this template's context values address.
    pub fn context_count(self) -> usize {
        match self {
            Self::Zero => 8192,
            Self::One => 1024,
        }
    }

    /// Context selecting the typical-prediction decision (T.88 section 6.3.5.6).
    ///
    /// It is the context of a pixel whose only set neighbour is the reference
    /// pixel at the same position, in this crate's context bit order: coding
    /// pixels, the coding adaptive pixel, then the reference pixels.
    pub fn typical_prediction_context(self) -> usize {
        match self {
            Self::Zero => 0x20,
            Self::One => 0x08,
        }
    }
}

/// A refinement template together with its resolved adaptive coordinates.
///
/// Index zero addresses the coding bitmap and index one the reference bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefinementTemplate {
    id: RefinementTemplateId,
    adaptive: [(i8, i8); REFINEMENT_ADAPTIVE],
}

impl RefinementTemplate {
    /// Read the adaptive coordinates a refinement header carries.
    pub(crate) fn parse(id: RefinementTemplateId, cursor: &mut ByteCursor<'_>) -> Result<Self> {
        let mut adaptive = [(-1_i8, -1_i8); REFINEMENT_ADAPTIVE];
        for index in 0..id.adaptive_count() {
            let x = cursor.u8()?;
            let y = cursor.u8()?;
            let slot = adaptive
                .get_mut(index)
                .ok_or(Jbig2Error::InvalidResource("refinement adaptive pixel"))?;
            *slot = (i8::from_ne_bytes([x]), i8::from_ne_bytes([y]));
        }
        Ok(Self { id, adaptive })
    }

    /// Return the validated template identifier.
    pub fn id(self) -> RefinementTemplateId {
        self.id
    }

    /// Return an adaptive pixel coordinate, or `None` outside the template.
    pub fn pixel(self, index: usize) -> Option<(i8, i8)> {
        if index >= self.id.adaptive_count() {
            return None;
        }
        self.adaptive.get(index).copied()
    }
}
