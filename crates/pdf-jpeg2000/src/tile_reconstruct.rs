//! Turning one tile's code-blocks into component samples.
//!
//! This is the second half of the decoding pipeline: inverse quantization of
//! every code-block's coefficients into the tile-component's coefficient
//! plane, the inverse wavelet transform of that plane, the inverse
//! multiple-component transform across the first three components, and the
//! DC level shift that returns each component to its declared range.
//!
//! A component's coefficients are integers when its wavelet filter is the
//! reversible 5/3 one and real numbers when it is the irreversible 9/7 one,
//! which is the pairing Part 1 defines. Both end as integers of the width the
//! tile was instantiated at.

use pdf_graphics::Size;

use crate::{
    Jpeg2000Error,
    code_block_decoder::{BlockRequest, CodeBlockDecoder},
    codestream::ComponentInfo,
    coding::WaveletTransform,
    coefficient::Coefficient,
    component_transform::{inverse_irreversible, inverse_reversible, level_shift},
    dequantize::{BandQuantization, Reconstruction},
    lifting::Lifting,
    region::Region,
    resolution::BandKind,
    tile_coding::TileCoding,
    tile_packets::TileBlocks,
    tile_structure::{BandRecord, ComponentLayout, TileStructure},
    wavelet,
    workspace::Workspace,
};

/// Amount SPcod and SPcoc subtract from each code-block exponent.
const CODE_BLOCK_EXPONENT_OFFSET: u32 = 2;
/// Components the multiple-component transform applies to.
const TRANSFORMED_COMPONENTS: usize = 3;

/// One reconstructed component of a tile, before colour conversion.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComponentOutput<C> {
    /// Zero-based SIZ component index.
    pub(crate) index: u16,
    /// Precision, signedness, and subsampling declared by SIZ.
    pub(crate) info: ComponentInfo,
    /// Extent of the component on its sampled grid.
    pub(crate) region: Region,
    /// Samples in raster order, one row after another.
    pub(crate) samples: Vec<C>,
}

/// Coefficients of one tile-component while they are being reconstructed.
#[derive(Debug)]
enum WorkingPlane<C> {
    /// A reversibly coded component, whose coefficients stay integers.
    Integer(Vec<C>),
    /// An irreversibly coded component, reconstructed in real numbers.
    Real(Vec<f64>),
}

impl<C: Coefficient + Lifting> WorkingPlane<C> {
    /// Allocates a plane of the size and kind one component needs.
    fn new(
        cells: usize,
        wavelet: WaveletTransform,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        match wavelet {
            WaveletTransform::Reversible53 => Ok(Self::Integer(workspace.vector(cells)?)),
            WaveletTransform::Irreversible97 => Ok(Self::Real(workspace.vector(cells)?)),
        }
    }

    /// Runs the inverse wavelet transform over the component's resolutions.
    fn transform(
        &mut self,
        stride: usize,
        resolutions: &[Region],
        integer_scratch: &mut [C],
        real_scratch: &mut [f64],
    ) -> Result<(), Jpeg2000Error> {
        match self {
            Self::Integer(plane) => {
                wavelet::reconstruct(plane, stride, resolutions, integer_scratch)
            }
            Self::Real(plane) => wavelet::reconstruct(plane, stride, resolutions, real_scratch),
        }
    }

    /// Converts the component to integer samples of the tile's width.
    fn into_samples(self) -> Vec<C> {
        match self {
            Self::Integer(plane) => plane,
            Self::Real(plane) => plane.into_iter().map(C::from_real).collect(),
        }
    }
}

/// Reconstructs every component of one tile.
///
/// # Errors
///
/// Returns a structural error for inconsistent coding parameters or corrupt
/// code-block data, and `LimitExceeded` when the tile's buffers pass the
/// caller's working-memory bound.
pub(crate) fn reconstruct<'a, C: Coefficient + Lifting>(
    structure: &TileStructure<'a>,
    blocks: &TileBlocks<'a>,
    coding: &TileCoding<'a, '_>,
    workspace: &mut Workspace,
) -> Result<Vec<ComponentOutput<C>>, Jpeg2000Error> {
    let mut decoder = CodeBlockDecoder::new(block_capacity(structure), workspace)?;
    let scratch = scratch_length(structure)?;
    let mut integer_scratch: Vec<C> = workspace.vector(scratch)?;
    let mut real_scratch: Vec<f64> = workspace.vector(scratch)?;
    let mut planes = Vec::new();
    for component in structure.components() {
        let cells = component.component.region.sample_count()?;
        let stride = usize::try_from(component.component.region.size().width).map_err(|_| {
            Jpeg2000Error::Overflow {
                context: "tile-component stride",
            }
        })?;
        let mut plane = WorkingPlane::new(cells, component.parameters.wavelet, workspace)?;
        fill(
            structure,
            blocks,
            component,
            &mut decoder,
            &mut plane,
            stride,
            workspace,
        )?;
        let resolutions: Vec<Region> = component
            .resolutions
            .iter()
            .map(|level| level.resolution.region())
            .collect();
        plane.transform(
            stride,
            &resolutions,
            &mut integer_scratch,
            &mut real_scratch,
        )?;
        workspace.push(&mut planes, plane)?;
    }
    if coding.style().component_transform {
        apply_component_transform(&mut planes)?;
    }
    finish(structure, planes)
}

/// Applies the inverse multiple-component transform to the first three
/// components of a tile.
fn apply_component_transform<C: Coefficient + Lifting>(
    planes: &mut [WorkingPlane<C>],
) -> Result<(), Jpeg2000Error> {
    let mismatch = || Jpeg2000Error::UnsupportedFeature {
        feature: "component transform over mixed wavelet filters",
    };
    let Some((transformed, _)) = planes.split_at_mut_checked(TRANSFORMED_COMPONENTS) else {
        return Err(mismatch());
    };
    if transformed
        .iter()
        .all(|plane| matches!(plane, WorkingPlane::Integer(_)))
    {
        let mut integers: Vec<&mut Vec<C>> = Vec::new();
        for plane in transformed.iter_mut() {
            match plane {
                WorkingPlane::Integer(values) => integers.push(values),
                WorkingPlane::Real(_) => return Err(mismatch()),
            }
        }
        return inverse_reversible_slices(&mut integers);
    }
    let mut reals: Vec<&mut Vec<f64>> = Vec::new();
    for plane in transformed.iter_mut() {
        match plane {
            WorkingPlane::Real(values) => reals.push(values),
            WorkingPlane::Integer(_) => return Err(mismatch()),
        }
    }
    inverse_irreversible_slices(&mut reals)
}

/// Runs the reversible inverse transform over three borrowed planes.
fn inverse_reversible_slices<C: Coefficient>(
    planes: &mut [&mut Vec<C>],
) -> Result<(), Jpeg2000Error> {
    let mut owned: Vec<Vec<C>> = planes
        .iter_mut()
        .map(|plane| core::mem::take(*plane))
        .collect();
    let result = inverse_reversible(&mut owned);
    restore(planes, owned);
    result
}

/// Runs the irreversible inverse transform over three borrowed planes.
fn inverse_irreversible_slices(planes: &mut [&mut Vec<f64>]) -> Result<(), Jpeg2000Error> {
    let mut owned: Vec<Vec<f64>> = planes
        .iter_mut()
        .map(|plane| core::mem::take(*plane))
        .collect();
    let result = inverse_irreversible(&mut owned);
    restore(planes, owned);
    result
}

/// Puts transformed planes back where they were borrowed from.
fn restore<T>(planes: &mut [&mut Vec<T>], owned: Vec<Vec<T>>) {
    for (slot, values) in planes.iter_mut().zip(owned) {
        **slot = values;
    }
}

/// Applies the DC level shift and pairs each plane with its component.
fn finish<C: Coefficient + Lifting>(
    structure: &TileStructure<'_>,
    planes: Vec<WorkingPlane<C>>,
) -> Result<Vec<ComponentOutput<C>>, Jpeg2000Error> {
    let mut outputs = Vec::new();
    for (component, plane) in structure.components().iter().zip(planes) {
        let mut samples = plane.into_samples();
        level_shift(&mut samples, component.component.info);
        outputs.push(ComponentOutput {
            index: component.component.index,
            info: component.component.info,
            region: component.component.region,
            samples,
        });
    }
    Ok(outputs)
}

/// Decodes every code-block of one component into its coefficient plane.
fn fill<'a, C: Coefficient + Lifting>(
    structure: &TileStructure<'a>,
    blocks: &TileBlocks<'a>,
    component: &ComponentLayout<'a>,
    decoder: &mut CodeBlockDecoder<'a>,
    plane: &mut WorkingPlane<C>,
    stride: usize,
    workspace: &mut Workspace,
) -> Result<(), Jpeg2000Error> {
    for band in structure.bands() {
        if band.component != component.component.index {
            continue;
        }
        fill_band(
            structure, blocks, component, band, decoder, plane, stride, workspace,
        )?;
    }
    Ok(())
}

/// Decodes every code-block of one subband into the component's plane.
#[allow(clippy::too_many_arguments)]
fn fill_band<'a, C: Coefficient + Lifting>(
    structure: &TileStructure<'a>,
    blocks: &TileBlocks<'a>,
    component: &ComponentLayout<'a>,
    band: &BandRecord,
    decoder: &mut CodeBlockDecoder<'a>,
    plane: &mut WorkingPlane<C>,
    stride: usize,
    workspace: &mut Workspace,
) -> Result<(), Jpeg2000Error> {
    let quantization = BandQuantization::new(&component.quantization, &band.subband)?;
    let roi_shift = component.roi_shift.unwrap_or(0);
    let magnitude_bits = quantization
        .magnitude_bits
        .saturating_add(u32::from(roi_shift));
    let origin = band_origin(structure, component, band)?;
    for slot in structure.band_slots(band) {
        for offset in 0..slot.block_count() {
            let index = slot
                .first_block
                .checked_add(offset)
                .and_then(|index| usize::try_from(index).ok())
                .ok_or(Jpeg2000Error::Overflow {
                    context: "code-block index",
                })?;
            let Some(region) = structure.regions().get(index).copied() else {
                continue;
            };
            let Some(state) = blocks.state(index) else {
                continue;
            };
            if region.is_empty() {
                continue;
            }
            let request = BlockRequest {
                size: region.size(),
                band: band.subband.kind,
                style: component.parameters.block_style,
                magnitude_bits,
                zero_bit_planes: state.zero_bit_planes,
                passes: state.passes,
            };
            let undecoded = decoder.decode(request, blocks.segments(state), workspace)?;
            let reconstruction = Reconstruction::new(
                quantization,
                undecoded,
                roi_shift,
                component.component.info.precision,
                band.subband.kind.gain(),
            );
            write_block(decoder, plane, stride, reconstruction, region, band, origin)?;
        }
    }
    Ok(())
}

/// Writes one decoded code-block's coefficients into the component's plane.
fn write_block<C: Coefficient + Lifting>(
    decoder: &CodeBlockDecoder<'_>,
    plane: &mut WorkingPlane<C>,
    stride: usize,
    reconstruction: Reconstruction,
    region: Region,
    band: &BandRecord,
    origin: Size<u32>,
) -> Result<(), Jpeg2000Error> {
    let overflow = || Jpeg2000Error::Overflow {
        context: "code-block placement",
    };
    let state = decoder.state();
    let size = region.size();
    let base = region.origin();
    let band_origin = band.subband.region.origin();
    let left = base
        .x
        .checked_sub(band_origin.x)
        .and_then(|value| value.checked_add(origin.width))
        .ok_or_else(overflow)?;
    let top = base
        .y
        .checked_sub(band_origin.y)
        .and_then(|value| value.checked_add(origin.height))
        .ok_or_else(overflow)?;
    for y in 0..size.height {
        for x in 0..size.width {
            let magnitude = *state
                .magnitudes()
                .get(
                    usize::try_from(y)
                        .ok()
                        .and_then(|row| row.checked_mul(usize::try_from(size.width).ok()?))
                        .and_then(|start| start.checked_add(usize::try_from(x).ok()?))
                        .ok_or_else(overflow)?,
                )
                .ok_or_else(overflow)?;
            if magnitude == 0 {
                continue;
            }
            let negative = state.is_negative(x, y);
            let column = usize::try_from(left.checked_add(x).ok_or_else(overflow)?)
                .map_err(|_| overflow())?;
            let row = usize::try_from(top.checked_add(y).ok_or_else(overflow)?)
                .map_err(|_| overflow())?;
            let position = row
                .checked_mul(stride)
                .and_then(|start| start.checked_add(column))
                .ok_or_else(overflow)?;
            match plane {
                WorkingPlane::Integer(values) => {
                    let slot = values.get_mut(position).ok_or_else(overflow)?;
                    *slot = reconstruction.integer(magnitude, negative)?;
                }
                WorkingPlane::Real(values) => {
                    let slot = values.get_mut(position).ok_or_else(overflow)?;
                    *slot = reconstruction.real(magnitude, negative);
                }
            }
        }
    }
    Ok(())
}

/// Returns where a subband's coefficients start in the component's plane.
///
/// Level zero occupies the upper-left corner; every higher level places its
/// horizontally high-pass subbands to the right of the level beneath it and
/// its vertically high-pass subbands below it.
fn band_origin(
    structure: &TileStructure<'_>,
    component: &ComponentLayout<'_>,
    band: &BandRecord,
) -> Result<Size<u32>, Jpeg2000Error> {
    let Some(previous) = band
        .resolution
        .checked_sub(1)
        .and_then(|level| structure.level(component.component.index, level))
    else {
        return Ok(Size {
            width: 0,
            height: 0,
        });
    };
    let extent = previous.resolution.region().size();
    Ok(Size {
        width: match band.subband.kind {
            BandKind::Hl | BandKind::Hh => extent.width,
            BandKind::Ll | BandKind::Lh => 0,
        },
        height: match band.subband.kind {
            BandKind::Lh | BandKind::Hh => extent.height,
            BandKind::Ll | BandKind::Hl => 0,
        },
    })
}

/// Returns the largest code-block extent any component of the tile uses.
fn block_capacity(structure: &TileStructure<'_>) -> Size<u32> {
    structure.components().iter().fold(
        Size {
            width: 1,
            height: 1,
        },
        |largest, component| {
            let exponents = component.parameters.code_block;
            let extent = |exponent: u8| {
                1u32.checked_shl(u32::from(exponent).saturating_add(CODE_BLOCK_EXPONENT_OFFSET))
                    .unwrap_or(1)
            };
            Size {
                width: largest.width.max(extent(exponents.width)),
                height: largest.height.max(extent(exponents.height)),
            }
        },
    )
}

/// Returns the longest signal the inverse wavelet transform will filter.
fn scratch_length(structure: &TileStructure<'_>) -> Result<usize, Jpeg2000Error> {
    let mut longest = 0u32;
    for component in structure.components() {
        let extent = component.component.region.size();
        longest = longest.max(extent.width).max(extent.height);
    }
    usize::try_from(longest).map_err(|_| Jpeg2000Error::Overflow {
        context: "inverse wavelet scratch length",
    })
}

/// Returns the widest code-block magnitude any subband of the tile allows.
///
/// The tile is reconstructed at the narrowest integer width that can hold
/// that magnitude and its sign.
///
/// # Errors
///
/// Returns a structural error if a quantization table omits a subband.
pub(crate) fn magnitude_bits(structure: &TileStructure<'_>) -> Result<u32, Jpeg2000Error> {
    let mut widest = 0u32;
    for band in structure.bands() {
        let Some(component) = structure.components().get(usize::from(band.component)) else {
            continue;
        };
        let quantization = BandQuantization::new(&component.quantization, &band.subband)?;
        let bits = quantization
            .magnitude_bits
            .saturating_add(u32::from(component.roi_shift.unwrap_or(0)));
        widest = widest.max(bits);
    }
    Ok(widest)
}
