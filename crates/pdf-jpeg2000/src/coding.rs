//! Part 1 coding-style, progression, and region-of-interest markers.
//!
//! COD carries the defaults for every tile and component, COC overrides one
//! component, POC changes the packet progression, and RGN scales a region of
//! interest. Their borrowed payloads are validated here and interpreted by the
//! tile decoder.

use bitflags::bitflags;
use pdf_graphics::Size;
use thiserror::Error;

use crate::{
    Jpeg2000Error,
    codestream::{Marker, MarkerReader, MarkerSegment},
};

/// Component count above which COC and QCC use a two-byte component index.
const WIDE_COMPONENT_INDEX: u16 = 257;
/// Largest Part 1 decomposition level count.
const MAX_LEVELS: u8 = 32;
/// Largest sum of the two code-block dimension exponent offsets.
const MAX_CODE_BLOCK_EXPONENT_SUM: u8 = 8;
/// The only region-of-interest style defined by Part 1.
const MAXSHIFT_STYLE: u8 = 0;

bitflags! {
    /// COD coding-style flags from Annex A.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct CodFlags: u8 {
        /// Precinct dimensions are explicitly supplied.
        const PRECINCTS = 1 << 0;
        /// SOP markers may appear in packet data.
        const SOP = 1 << 1;
        /// EPH markers may appear after packet headers.
        const EPH = 1 << 2;
    }

    /// Part 1 code-block style flags.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) struct CodeBlockFlags: u8 {
        /// Selective arithmetic bypass.
        const BYPASS = 1 << 0;
        /// Reset contexts after each pass.
        const RESET = 1 << 1;
        /// Terminate each pass.
        const TERMINATE = 1 << 2;
        /// Vertically causal contexts.
        const VERTICAL_CAUSAL = 1 << 3;
        /// Predictable termination.
        const PREDICTABLE = 1 << 4;
        /// Segmentation symbol.
        const SEGMENTATION = 1 << 5;
    }
}

/// Malformed COD or COC syntax.
#[derive(Debug, Error)]
pub enum CodingError {
    /// A coding-style segment has too few fields.
    #[error("coding style at byte {0} is incomplete")]
    Short(usize),
    /// A coding-style segment uses a reserved style bit.
    #[error("coding style at byte {0} has a reserved style flag")]
    Flags(usize),
    /// Coding parameters or the precinct count are invalid.
    #[error("coding style at byte {0} has invalid coding parameters")]
    Parameters(usize),
    /// The progression order is outside the Part 1 set.
    #[error("unknown progression order {order} at byte {offset}")]
    Progression {
        /// Byte offset of the segment.
        offset: usize,
        /// Raw SGcod progression value.
        order: u8,
    },
    /// The wavelet transform is outside the Part 1 set.
    #[error("unknown wavelet transform {transform} at byte {offset}")]
    Wavelet {
        /// Byte offset of the segment.
        offset: usize,
        /// Raw SPcod transform value.
        transform: u8,
    },
    /// A COC segment names a component outside the SIZ table.
    #[error("coding style at byte {0} names an unknown component")]
    Component(usize),
}

/// Order in which packets progress through the codestream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProgressionOrder {
    /// Layer, resolution, component, position.
    LayerResolutionComponentPosition,
    /// Resolution, layer, component, position.
    ResolutionLayerComponentPosition,
    /// Resolution, position, component, layer.
    ResolutionPositionComponentLayer,
    /// Position, component, resolution, layer.
    PositionComponentResolutionLayer,
    /// Component, position, resolution, layer.
    ComponentPositionResolutionLayer,
}

impl TryFrom<u8> for ProgressionOrder {
    type Error = ();

    fn try_from(order: u8) -> Result<Self, Self::Error> {
        match order {
            0 => Ok(Self::LayerResolutionComponentPosition),
            1 => Ok(Self::ResolutionLayerComponentPosition),
            2 => Ok(Self::ResolutionPositionComponentLayer),
            3 => Ok(Self::PositionComponentResolutionLayer),
            4 => Ok(Self::ComponentPositionResolutionLayer),
            _ => Err(()),
        }
    }
}

/// Wavelet filter applied by the inverse transform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WaveletTransform {
    /// Irreversible 9/7 filter.
    Irreversible97,
    /// Reversible 5/3 filter.
    Reversible53,
}

impl TryFrom<u8> for WaveletTransform {
    type Error = ();

    fn try_from(transform: u8) -> Result<Self, Self::Error> {
        match transform {
            0 => Ok(Self::Irreversible97),
            1 => Ok(Self::Reversible53),
            _ => Err(()),
        }
    }
}

/// Decomposition and code-block parameters shared by COD and COC.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CodingParameters<'a> {
    /// Number of wavelet decomposition levels.
    pub(crate) levels: u8,
    /// Code-block width and height exponents, less two.
    pub(crate) code_block: Size<u8>,
    /// Code-block coding style options.
    pub(crate) block_style: CodeBlockFlags,
    /// Wavelet filter used by the inverse transform.
    pub(crate) wavelet: WaveletTransform,
    /// Borrowed precinct size bytes, one per resolution level.
    pub(crate) precincts: Option<&'a [u8]>,
}

impl<'a> CodingParameters<'a> {
    /// Reads SPcod or SPcoc, including explicit precinct sizes.
    fn parse(
        bytes: &'a [u8],
        explicit_precincts: bool,
        offset: usize,
    ) -> Result<Self, CodingError> {
        let [levels, width, height, block_style, transform, rest @ ..] = bytes else {
            return Err(CodingError::Short(offset));
        };
        let wavelet =
            WaveletTransform::try_from(*transform).map_err(|()| CodingError::Wavelet {
                offset,
                transform: *transform,
            })?;
        let Some(block_style) = CodeBlockFlags::from_bits(*block_style) else {
            return Err(CodingError::Flags(offset));
        };
        let precincts = if explicit_precincts {
            let expected = usize::from(*levels).saturating_add(1);
            if rest.len() != expected {
                return Err(CodingError::Parameters(offset));
            }
            Some(rest)
        } else {
            if !rest.is_empty() {
                return Err(CodingError::Parameters(offset));
            }
            None
        };
        if *levels > MAX_LEVELS || width.saturating_add(*height) > MAX_CODE_BLOCK_EXPONENT_SUM {
            return Err(CodingError::Parameters(offset));
        }
        Ok(Self {
            levels: *levels,
            code_block: Size {
                width: *width,
                height: *height,
            },
            block_style,
            wavelet,
            precincts,
        })
    }

    /// Returns the number of subbands described by a quantization segment.
    pub(crate) fn subband_count(&self) -> usize {
        usize::from(self.levels).saturating_mul(3).saturating_add(1)
    }
}

/// Validated COD defaults for every tile and component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CodingStyle<'a> {
    /// Packet progression order.
    pub(crate) progression: ProgressionOrder,
    /// Number of quality layers.
    pub(crate) layers: u16,
    /// Whether a multiple-component transform is applied.
    pub(crate) component_transform: bool,
    /// Whether SOP markers may precede packets.
    pub(crate) sop: bool,
    /// Whether EPH markers may follow packet headers.
    pub(crate) eph: bool,
    /// Decomposition and code-block parameters.
    pub(crate) parameters: CodingParameters<'a>,
}

impl<'a> TryFrom<MarkerSegment<'a>> for CodingStyle<'a> {
    type Error = CodingError;

    fn try_from(segment: MarkerSegment<'a>) -> Result<Self, Self::Error> {
        let offset = segment.offset();
        let [
            style,
            progression,
            layers_hi,
            layers_lo,
            transform,
            rest @ ..,
        ] = segment.payload()
        else {
            return Err(CodingError::Short(offset));
        };
        let Some(flags) = CodFlags::from_bits(*style) else {
            return Err(CodingError::Flags(offset));
        };
        let progression =
            ProgressionOrder::try_from(*progression).map_err(|()| CodingError::Progression {
                offset,
                order: *progression,
            })?;
        let layers = u16::from_be_bytes([*layers_hi, *layers_lo]);
        if layers == 0 || *transform > 1 {
            return Err(CodingError::Parameters(offset));
        }
        Ok(Self {
            progression,
            layers,
            component_transform: *transform == 1,
            sop: flags.contains(CodFlags::SOP),
            eph: flags.contains(CodFlags::EPH),
            parameters: CodingParameters::parse(rest, flags.contains(CodFlags::PRECINCTS), offset)?,
        })
    }
}

/// A COC coding-style override for one component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ComponentCodingStyle<'a> {
    /// Zero-based SIZ component index.
    pub(crate) component: u16,
    /// Decomposition and code-block parameters replacing the COD defaults.
    pub(crate) parameters: CodingParameters<'a>,
}

impl<'a> ComponentCodingStyle<'a> {
    /// Reads a COC segment and checks its component against SIZ.
    pub(crate) fn parse(segment: MarkerSegment<'a>, components: u16) -> Result<Self, CodingError> {
        let offset = segment.offset();
        let (component, rest) = split_component_index(segment.payload(), components, offset)?;
        let Some((&style, parameters)) = rest.split_first() else {
            return Err(CodingError::Short(offset));
        };
        let Some(flags) = CodFlags::from_bits(style) else {
            return Err(CodingError::Flags(offset));
        };
        if flags.difference(CodFlags::PRECINCTS) != CodFlags::empty() {
            return Err(CodingError::Flags(offset));
        }
        Ok(Self {
            component,
            parameters: CodingParameters::parse(
                parameters,
                flags.contains(CodFlags::PRECINCTS),
                offset,
            )?,
        })
    }
}

impl<'a> ComponentCodingStyle<'a> {
    /// Returns the COC override for one component in a marker sequence.
    ///
    /// The same scan serves the main header and a tile-part header, so an
    /// override is never copied while its segment is first read.
    pub(crate) fn find(
        mut markers: MarkerReader<'a>,
        component: u16,
        components: u16,
    ) -> Result<Option<CodingParameters<'a>>, Jpeg2000Error> {
        while let Some(segment) = markers.next_segment()? {
            if segment.marker() != Marker::Coc {
                continue;
            }
            let style = Self::parse(segment, components)?;
            if style.component == component {
                return Ok(Some(style.parameters));
            }
        }
        Ok(None)
    }
}

/// Reads the component index that opens a COC or QCC segment.
///
/// Part 1 widens the field to two bytes once SIZ declares 257 components.
pub(crate) fn split_component_index(
    payload: &[u8],
    components: u16,
    offset: usize,
) -> Result<(u16, &[u8]), CodingError> {
    let (component, rest) = if components < WIDE_COMPONENT_INDEX {
        let Some((&index, rest)) = payload.split_first() else {
            return Err(CodingError::Short(offset));
        };
        (u16::from(index), rest)
    } else {
        let [high, low, rest @ ..] = payload else {
            return Err(CodingError::Short(offset));
        };
        (u16::from_be_bytes([*high, *low]), rest)
    };
    if component >= components {
        return Err(CodingError::Component(offset));
    }
    Ok((component, rest))
}

/// One POC record: a progression order over a range of the packet axes.
///
/// Layers always start at zero, so a record carries only the end layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProgressionChange {
    /// Order applied while the record is in force.
    pub(crate) order: ProgressionOrder,
    /// First resolution level covered by the record.
    pub(crate) start_resolution: u8,
    /// Resolution level just past the record's range.
    pub(crate) end_resolution: u8,
    /// First component covered by the record.
    pub(crate) start_component: u16,
    /// Component just past the record's range.
    pub(crate) end_component: u16,
    /// Quality layer just past the record's range.
    pub(crate) end_layer: u16,
}

impl ProgressionChange {
    /// Reads one POC record and clamps its saturated end values.
    ///
    /// Part 1 encoders commonly signal "every remaining resolution,
    /// component, or layer" with a value at or beyond the axis length, so the
    /// end fields are clamped rather than rejected.
    fn read(record: &[u8], limits: &ProgressionLimits, offset: usize) -> Result<Self, CodingError> {
        let [start_resolution, rest @ ..] = record else {
            return Err(CodingError::Short(offset));
        };
        let (start_component, rest) = split_component_index(rest, limits.components, offset)?;
        let [layer_hi, layer_lo, end_resolution, rest @ ..] = rest else {
            return Err(CodingError::Short(offset));
        };
        let (end_component, [order]) = split_component_index_open(rest, limits.components, offset)?
        else {
            return Err(CodingError::Short(offset));
        };
        let order = ProgressionOrder::try_from(*order).map_err(|()| CodingError::Progression {
            offset,
            order: *order,
        })?;
        let end_resolution = (*end_resolution).min(limits.resolutions);
        let end_layer = u16::from_be_bytes([*layer_hi, *layer_lo]).min(limits.layers);
        if *start_resolution >= end_resolution || start_component >= end_component || end_layer == 0
        {
            return Err(CodingError::Parameters(offset));
        }
        Ok(Self {
            order,
            start_resolution: *start_resolution,
            end_resolution,
            start_component,
            end_component,
            end_layer,
        })
    }
}

/// Axis lengths a POC record's end values are clamped against.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProgressionLimits {
    components: u16,
    resolutions: u8,
    layers: u16,
}

/// A POC segment's progression-order changes.
///
/// The records stay borrowed and are re-read when the packet sequence is
/// built, so a change is validated once without being copied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProgressionChanges<'a> {
    records: &'a [u8],
    record_bytes: usize,
    limits: ProgressionLimits,
    offset: usize,
}

impl<'a> ProgressionChanges<'a> {
    /// Checks every record's resolution, component, and layer ordering.
    pub(crate) fn parse(
        segment: MarkerSegment<'a>,
        components: u16,
        style: &CodingStyle<'_>,
    ) -> Result<Self, CodingError> {
        let offset = segment.offset();
        let records = segment.payload();
        let index_bytes: usize = if components < WIDE_COMPONENT_INDEX {
            1
        } else {
            2
        };
        let record_bytes = 5usize.saturating_add(index_bytes.saturating_mul(2));
        if records.is_empty() || !records.len().is_multiple_of(record_bytes) {
            return Err(CodingError::Parameters(offset));
        }
        let changes = Self {
            records,
            record_bytes,
            limits: ProgressionLimits {
                components,
                resolutions: style.parameters.levels.saturating_add(1),
                layers: style.layers,
            },
            offset,
        };
        for record in records.chunks_exact(record_bytes) {
            ProgressionChange::read(record, &changes.limits, offset)?;
        }
        Ok(changes)
    }

    /// Returns every validated record in codestream order.
    pub(crate) fn changes(&self) -> impl Iterator<Item = ProgressionChange> + '_ {
        self.records
            .chunks_exact(self.record_bytes)
            .filter_map(move |record| {
                ProgressionChange::read(record, &self.limits, self.offset).ok()
            })
    }
}

/// Reads a POC end-component index, which saturates to every component.
fn split_component_index_open(
    payload: &[u8],
    components: u16,
    offset: usize,
) -> Result<(u16, &[u8]), CodingError> {
    let (index, rest) = if components < WIDE_COMPONENT_INDEX {
        let [index, rest @ ..] = payload else {
            return Err(CodingError::Short(offset));
        };
        (u16::from(*index), rest)
    } else {
        let [high, low, rest @ ..] = payload else {
            return Err(CodingError::Short(offset));
        };
        (u16::from_be_bytes([*high, *low]), rest)
    };
    // Zero means "through the last component", and a value beyond the last
    // component is clamped the way Part 1 decoders accept it in practice.
    let index = if index == 0 {
        components
    } else {
        index.min(components)
    };
    Ok((index, rest))
}

/// A validated RGN region-of-interest segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RegionOfInterest {
    /// Component the region applies to.
    pub(crate) component: u16,
    /// Implicit shift applied to region coefficients.
    pub(crate) shift: u8,
}

impl RegionOfInterest {
    /// Reads an RGN segment, which Part 1 limits to the Maxshift style.
    pub(crate) fn parse(segment: MarkerSegment<'_>, components: u16) -> Result<Self, CodingError> {
        let offset = segment.offset();
        let (component, rest) = split_component_index(segment.payload(), components, offset)?;
        let [MAXSHIFT_STYLE, shift] = rest else {
            return Err(CodingError::Parameters(offset));
        };
        Ok(Self {
            component,
            shift: *shift,
        })
    }
}
