//! Part 1 quantization defaults from QCD and per-component QCC overrides.

use thiserror::Error;

use crate::{
    codestream::MarkerSegment,
    coding::{CodingError, CodingParameters, split_component_index},
    marker_site::MarkerSite,
};

/// Mask selecting the quantization style from Sqcd or Sqcc.
const STYLE_MASK: u8 = 0x1f;
/// Bits holding the guard-bit count in Sqcd or Sqcc.
const GUARD_BITS_SHIFT: u32 = 5;
/// No quantization; one exponent byte per subband.
const NO_QUANTIZATION: u8 = 0;
/// Derived scalar quantization; one two-byte step size.
const SCALAR_DERIVED: u8 = 1;
/// Expounded scalar quantization; one two-byte step size per subband.
const SCALAR_EXPOUNDED: u8 = 2;
/// Bytes in one scalar step-size field.
const STEP_BYTES: usize = 2;

/// Malformed QCD or QCC syntax.
#[derive(Debug, Error)]
pub enum QuantizationError {
    /// The segment is missing Sqcd or Sqcc.
    #[error("quantization at byte {0} is incomplete")]
    Short(usize),
    /// The segment requests an unknown quantization style.
    #[error("quantization at byte {0} has an unknown style")]
    Style(usize),
    /// The step-size count does not match the decomposition levels.
    #[error("quantization at byte {0} has the wrong step-size count")]
    Length(usize),
    /// The component index or its field layout is invalid.
    #[error(transparent)]
    Component(#[from] CodingError),
}

/// How quantization step sizes are signalled for the subbands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuantizationStyle {
    /// Reversible path: one exponent per subband and no quantization.
    None,
    /// One step size, from which every subband's value is derived.
    Derived,
    /// One step size per subband.
    Expounded,
}

impl QuantizationStyle {
    /// Returns the byte count that this style requires for the subbands.
    fn expected_len(self, subbands: usize) -> usize {
        match self {
            Self::None => subbands,
            Self::Derived => STEP_BYTES,
            Self::Expounded => subbands.saturating_mul(STEP_BYTES),
        }
    }
}

impl TryFrom<u8> for QuantizationStyle {
    type Error = ();

    fn try_from(style: u8) -> Result<Self, Self::Error> {
        match style & STYLE_MASK {
            NO_QUANTIZATION => Ok(Self::None),
            SCALAR_DERIVED => Ok(Self::Derived),
            SCALAR_EXPOUNDED => Ok(Self::Expounded),
            _ => Err(()),
        }
    }
}

/// Validated quantization parameters with their borrowed step-size table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Quantization<'a> {
    /// How the step sizes are signalled.
    pub(crate) style: QuantizationStyle,
    /// Guard bits protecting the magnitude range.
    pub(crate) guard_bits: u8,
    /// Borrowed exponent or step-size bytes.
    pub(crate) steps: &'a [u8],
    site: MarkerSite,
}

impl<'a> Quantization<'a> {
    /// Reads a QCD segment carrying the codestream defaults.
    pub(crate) fn parse(segment: MarkerSegment<'a>) -> Result<Self, QuantizationError> {
        Self::from_fields(segment.payload(), segment.site())
    }

    /// Reads Sqcd or Sqcc and retains the step-size table.
    fn from_fields(payload: &'a [u8], site: MarkerSite) -> Result<Self, QuantizationError> {
        let Some((&style, steps)) = payload.split_first() else {
            return Err(QuantizationError::Short(site.offset()));
        };
        let Ok(parsed) = QuantizationStyle::try_from(style) else {
            return Err(QuantizationError::Style(site.offset()));
        };
        Ok(Self {
            style: parsed,
            guard_bits: style >> GUARD_BITS_SHIFT,
            steps,
            site,
        })
    }

    /// Returns the source marker for diagnostics about this step-size table.
    pub(crate) fn site(&self) -> MarkerSite {
        self.site
    }

    /// Checks the step-size count against the decomposition it belongs to.
    pub(crate) fn validate_subbands(
        &self,
        parameters: &CodingParameters<'_>,
    ) -> Result<(), QuantizationError> {
        if self.steps.len() == self.style.expected_len(parameters.subband_count()) {
            return Ok(());
        }
        Err(QuantizationError::Length(self.site.offset()))
    }
}

/// A QCC quantization override for one component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ComponentQuantization<'a> {
    /// Zero-based SIZ component index.
    pub(crate) component: u16,
    /// Quantization parameters replacing the QCD defaults.
    pub(crate) quantization: Quantization<'a>,
}

impl<'a> ComponentQuantization<'a> {
    /// Reads a QCC segment and checks its component against SIZ.
    pub(crate) fn parse(
        segment: MarkerSegment<'a>,
        components: u16,
    ) -> Result<Self, QuantizationError> {
        let offset = segment.offset();
        let (component, rest) = split_component_index(segment.payload(), components, offset)?;
        Ok(Self {
            component,
            quantization: Quantization::from_fields(rest, segment.site())?,
        })
    }
}
