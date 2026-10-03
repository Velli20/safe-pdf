//! The code-block layout of one tile, built before any packet is read.
//!
//! Annex B nests four partitions inside a tile: components, resolution
//! levels, precincts, and code-blocks. This module walks that nesting once and
//! records the geometry it produces. Nothing here changes while packets are
//! read, so the layout stays shared and immutable and the per-code-block state
//! lives beside it in [`crate::tile_packets`].
//!
//! Code-blocks and precinct slots are both stored precinct-major within a
//! resolution level, which makes the subband slots of one precinct — exactly
//! what one packet describes — a contiguous run.

use crate::{
    Jpeg2000Error,
    code_block_grid::CodeBlockGrid,
    coding::CodingParameters,
    precinct::{PrecinctGrid, PrecinctSizes},
    quantization::Quantization,
    region::Region,
    resolution::{Resolution, Subband},
    tile_coding::TileCoding,
    tile_layout::{TileComponent, TileLayout},
    workspace::Workspace,
};

/// The code-block partition of one subband inside one precinct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SlotLayout {
    /// Code-block partition of the subband precinct.
    pub(crate) grid: CodeBlockGrid,
    /// Index of the slot's first code-block in the tile's block table.
    pub(crate) first_block: u32,
}

impl SlotLayout {
    /// Returns the number of code-blocks the slot carries.
    pub(crate) fn block_count(self) -> u32 {
        u32::try_from(self.grid.count()).unwrap_or(u32::MAX)
    }
}

/// One subband of one resolution level of one tile-component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BandRecord {
    /// Zero-based SIZ component index.
    pub(crate) component: u16,
    /// Resolution level the subband belongs to.
    pub(crate) resolution: u8,
    /// Position of the subband within its resolution level.
    pub(crate) band: u32,
    /// Geometry and quantization position of the subband.
    pub(crate) subband: Subband,
}

/// Resolution level of one tile-component, with its precinct partition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolutionLayout {
    /// Geometry of the level on the resolution grid.
    pub(crate) resolution: Resolution,
    /// Precinct partition of the level.
    pub(crate) precincts: PrecinctGrid,
    /// Number of subbands the level carries.
    pub(crate) band_count: u32,
    /// Number of precincts the level carries.
    pub(crate) precinct_count: u32,
    /// Index of the level's first precinct slot in the tile's slot table.
    pub(crate) first_slot: u32,
}

impl ResolutionLayout {
    /// Returns the slot range describing one precinct of this level.
    ///
    /// The range covers every subband of the precinct, which is the set of
    /// code-blocks one packet can name.
    pub(crate) fn precinct_slots(&self, precinct: u32) -> Option<core::ops::Range<usize>> {
        if precinct >= self.precinct_count {
            return None;
        }
        let start = precinct
            .checked_mul(self.band_count)?
            .checked_add(self.first_slot)?;
        let end = start.checked_add(self.band_count)?;
        Some(usize::try_from(start).ok()?..usize::try_from(end).ok()?)
    }

    /// Returns the slot index of one subband precinct of this level.
    fn slot(&self, band: u32, precinct: u32) -> Option<usize> {
        if band >= self.band_count {
            return None;
        }
        let start = self.precinct_slots(precinct)?.start;
        usize::try_from(band).ok()?.checked_add(start)
    }
}

/// One component of a tile, with the parameters governing its coefficients.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComponentLayout<'a> {
    /// Sampled-grid geometry and SIZ descriptor of the component.
    pub(crate) component: TileComponent,
    /// Decomposition and code-block parameters in force for the component.
    pub(crate) parameters: CodingParameters<'a>,
    /// Quantization parameters in force for the component.
    pub(crate) quantization: Quantization<'a>,
    /// Maxshift value of the component's region of interest, if any.
    pub(crate) roi_shift: Option<u8>,
    /// Resolution levels from zero to the component's decomposition count.
    pub(crate) resolutions: Vec<ResolutionLayout>,
}

/// The geometry of every code-block in one tile.
#[derive(Debug)]
pub(crate) struct TileStructure<'a> {
    region: Region,
    components: Vec<ComponentLayout<'a>>,
    bands: Vec<BandRecord>,
    slots: Vec<SlotLayout>,
    regions: Vec<Region>,
}

impl<'a> TileStructure<'a> {
    /// Walks the tile's partitions and records every code-block's geometry.
    ///
    /// # Errors
    ///
    /// Returns a structural error for invalid coding parameters and
    /// `LimitExceeded` when the tables pass the caller's working-memory bound.
    pub(crate) fn build(
        layout: &TileLayout<'a>,
        coding: &TileCoding<'a, '_>,
        workspace: &mut Workspace,
    ) -> Result<Self, Jpeg2000Error> {
        let mut structure = Self {
            region: layout.region(),
            components: Vec::new(),
            bands: Vec::new(),
            slots: Vec::new(),
            regions: Vec::new(),
        };
        for component in layout.components() {
            let entry = structure.build_component(component, coding, workspace)?;
            workspace.push(&mut structure.components, entry)?;
        }
        Ok(structure)
    }

    /// Returns the tile's extent on the reference grid.
    pub(crate) fn tile_region(&self) -> Region {
        self.region
    }

    /// Returns the tile's components in SIZ order.
    pub(crate) fn components(&self) -> &[ComponentLayout<'a>] {
        &self.components
    }

    /// Returns every subband of the tile, in component and resolution order.
    pub(crate) fn bands(&self) -> &[BandRecord] {
        &self.bands
    }

    /// Returns every code-block region of the tile, in block-table order.
    pub(crate) fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// Returns the number of code-blocks in the tile.
    pub(crate) fn block_count(&self) -> usize {
        self.regions.len()
    }

    /// Returns the number of subband precincts in the tile.
    pub(crate) fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Returns one subband precinct's code-block partition.
    pub(crate) fn slot(&self, index: usize) -> Option<SlotLayout> {
        self.slots.get(index).copied()
    }

    /// Returns one component's resolution level.
    pub(crate) fn level(&self, component: u16, resolution: u8) -> Option<&ResolutionLayout> {
        self.components
            .get(usize::from(component))?
            .resolutions
            .get(usize::from(resolution))
    }

    /// Returns the slots of one subband, one per precinct of its level.
    pub(crate) fn band_slots(&self, band: &BandRecord) -> impl Iterator<Item = SlotLayout> {
        let level = self.level(band.component, band.resolution);
        let count = level.map_or(0, |level| level.precinct_count);
        (0..count).filter_map(move |precinct| {
            let index = level?.slot(band.band, precinct)?;
            self.slots.get(index).copied()
        })
    }

    /// Builds the resolution levels and code-blocks of one component.
    fn build_component(
        &mut self,
        component: TileComponent,
        coding: &TileCoding<'a, '_>,
        workspace: &mut Workspace,
    ) -> Result<ComponentLayout<'a>, Jpeg2000Error> {
        let parameters = coding.component(component.index)?;
        let sizes = PrecinctSizes::new(&parameters);
        let mut resolutions = Vec::new();
        for index in 0..=parameters.levels {
            let resolution = Resolution::new(component.region, parameters.levels, index)?;
            let precincts = PrecinctGrid::new(resolution, &sizes)?;
            let level = self.build_resolution(
                component.index,
                resolution,
                precincts,
                &parameters,
                workspace,
            )?;
            workspace.push(&mut resolutions, level)?;
        }
        Ok(ComponentLayout {
            component,
            parameters,
            quantization: coding.quantization(component.index)?,
            roi_shift: coding.region_of_interest(component.index)?,
            resolutions,
        })
    }

    /// Builds the precinct slots and code-blocks of one resolution level.
    fn build_resolution(
        &mut self,
        component: u16,
        resolution: Resolution,
        precincts: PrecinctGrid,
        parameters: &CodingParameters<'a>,
        workspace: &mut Workspace,
    ) -> Result<ResolutionLayout, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "tile code-block table",
        };
        let bands = resolution.bands()?;
        let precinct_count = u32::try_from(precincts.count()).map_err(|_| overflow())?;
        let band_count = u32::try_from(bands.as_slice().len()).map_err(|_| overflow())?;
        let first_slot = u32::try_from(self.slots.len()).map_err(|_| overflow())?;
        for (index, band) in bands.as_slice().iter().enumerate() {
            let record = BandRecord {
                component,
                resolution: resolution.index(),
                band: u32::try_from(index).map_err(|_| overflow())?,
                subband: *band,
            };
            workspace.push(&mut self.bands, record)?;
        }
        for precinct in 0..precinct_count {
            for band in bands.as_slice() {
                self.build_slot(&precincts, band, precinct, parameters, workspace)?;
            }
        }
        Ok(ResolutionLayout {
            resolution,
            precincts,
            band_count,
            precinct_count,
            first_slot,
        })
    }

    /// Builds one subband precinct's code-block partition and regions.
    fn build_slot(
        &mut self,
        precincts: &PrecinctGrid,
        band: &Subband,
        index: u32,
        parameters: &CodingParameters<'a>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "tile code-block table",
        };
        let precinct = precincts.precinct(index).ok_or_else(overflow)?;
        let region = precincts.band_region(band, &precinct)?;
        let grid = CodeBlockGrid::new(region, parameters, precincts.band_exponents())?;
        let first_block = u32::try_from(self.regions.len()).map_err(|_| overflow())?;
        for block in grid.blocks() {
            workspace.push(&mut self.regions, block.region)?;
        }
        workspace.push(&mut self.slots, SlotLayout { grid, first_block })
    }
}
