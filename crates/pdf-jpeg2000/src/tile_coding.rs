//! Coding and quantization parameters in force for one tile.
//!
//! Annex A.6 layers four sources over each other. From most to least
//! specific they are a COC or QCC in the tile-part header, a COD or QCD in the
//! tile-part header, a COC or QCC in the main header, and finally the main
//! COD or QCD. A region of interest follows the same rule with RGN, and a POC
//! in any tile-part header replaces the main header's progression.
//!
//! Nothing is copied: the resolution re-reads the borrowed marker segments
//! each time a component is asked for, as the main header already does.

use core::ops::Range;

use crate::{
    Jpeg2000Error,
    codestream::{MainHeader, Marker, MarkerReader, TilePart},
    coding::{
        CodingParameters, CodingStyle, ComponentCodingStyle, ProgressionChange, ProgressionChanges,
        RegionOfInterest,
    },
    progression::ProgressionStep,
    quantization::{ComponentQuantization, Quantization},
    tile_part::TilePartIndex,
    workspace::Workspace,
};

/// The coding parameters that apply to one tile of a codestream.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TileCoding<'a, 'index> {
    main: &'index MainHeader<'a>,
    index: &'index TilePartIndex<'a>,
    tile: u32,
    style: CodingStyle<'a>,
    quantization: Quantization<'a>,
    overridden: bool,
}

impl<'a, 'index> TileCoding<'a, 'index> {
    /// Resolves the tile defaults from the main header and tile-part headers.
    ///
    /// # Errors
    ///
    /// Returns a structural error if a tile-part header override is malformed.
    pub(crate) fn resolve(
        main: &'index MainHeader<'a>,
        index: &'index TilePartIndex<'a>,
        tile: u32,
    ) -> Result<Self, Jpeg2000Error> {
        let mut style = None;
        let mut quantization = None;
        if let Some(part) = index.tile_parts(tile).next() {
            let mut markers = Self::markers(part);
            while let Some(segment) = markers.next_segment()? {
                match segment.marker() {
                    Marker::Cod => style = Some(CodingStyle::try_from(segment)?),
                    Marker::Qcd => quantization = Some(Quantization::parse(segment)?),
                    _ => {}
                }
            }
        }
        Ok(Self {
            main,
            index,
            tile,
            overridden: style.is_some(),
            style: style.unwrap_or_else(|| *main.coding()),
            quantization: quantization.unwrap_or(main.quantization),
        })
    }

    /// Returns the coding style governing the tile's packets.
    pub(crate) fn style(&self) -> &CodingStyle<'a> {
        &self.style
    }

    /// Returns the decomposition and code-block parameters of one component.
    ///
    /// # Errors
    ///
    /// Returns a structural error if an override segment is malformed.
    pub(crate) fn component(&self, component: u16) -> Result<CodingParameters<'a>, Jpeg2000Error> {
        let components = self.main.size().component_count();
        if let Some(part) = self.index.tile_parts(self.tile).next()
            && let Some(parameters) =
                ComponentCodingStyle::find(Self::markers(part), component, components)?
        {
            return Ok(parameters);
        }
        if self.overridden {
            return Ok(self.style.parameters);
        }
        self.main.component_parameters(component)
    }

    /// Returns the quantization parameters of one component.
    ///
    /// # Errors
    ///
    /// Returns a structural error if an override segment is malformed.
    pub(crate) fn quantization(&self, component: u16) -> Result<Quantization<'a>, Jpeg2000Error> {
        let components = self.main.size().component_count();
        if let Some(part) = self.index.tile_parts(self.tile).next() {
            let mut markers = Self::markers(part);
            while let Some(segment) = markers.next_segment()? {
                if segment.marker() != Marker::Qcc {
                    continue;
                }
                let found = ComponentQuantization::parse(segment, components)?;
                if found.component == component {
                    return Ok(found.quantization);
                }
            }
        }
        if let Some(found) = self.main_quantization(component)? {
            return Ok(found);
        }
        Ok(self.quantization)
    }

    /// Returns the Maxshift value of a component's region of interest.
    ///
    /// # Errors
    ///
    /// Returns a structural error if an RGN segment is malformed.
    pub(crate) fn region_of_interest(&self, component: u16) -> Result<Option<u8>, Jpeg2000Error> {
        let components = self.main.size().component_count();
        if let Some(part) = self.index.tile_parts(self.tile).next()
            && let Some(shift) = Self::find_region(Self::markers(part), component, components)?
        {
            return Ok(Some(shift));
        }
        Self::find_region(self.main.markers(), component, components)
    }

    /// Appends the progression steps that order the tile's packets.
    ///
    /// A POC segment in any tile-part header, or failing that in the main
    /// header, replaces the COD order with its own records.
    ///
    /// # Errors
    ///
    /// Returns a structural error if a POC segment is malformed, and
    /// `LimitExceeded` when the step list passes the caller's bound.
    pub(crate) fn progression_steps(
        &self,
        components: Range<u16>,
        resolutions: Range<u8>,
        layers: Range<u16>,
        into: &mut Vec<ProgressionStep>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        for part in self.index.tile_parts(self.tile) {
            self.append_changes(Self::markers(part), into, workspace)?;
        }
        if into.is_empty() {
            self.append_changes(self.main.markers(), into, workspace)?;
        }
        if into.is_empty() {
            workspace.push(
                into,
                ProgressionStep {
                    order: self.style.progression,
                    resolutions,
                    components,
                    layers,
                },
            )?;
        }
        Ok(())
    }

    /// Appends one marker sequence's POC records as progression steps.
    fn append_changes(
        &self,
        mut markers: MarkerReader<'a>,
        into: &mut Vec<ProgressionStep>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        let components = self.main.size().component_count();
        while let Some(segment) = markers.next_segment()? {
            if segment.marker() != Marker::Poc {
                continue;
            }
            let changes = ProgressionChanges::parse(segment, components, &self.style)?;
            for change in changes.changes() {
                workspace.push(into, Self::step(&change))?;
            }
        }
        Ok(())
    }

    /// Converts one POC record into a progression step.
    fn step(change: &ProgressionChange) -> ProgressionStep {
        ProgressionStep {
            order: change.order,
            resolutions: change.start_resolution..change.end_resolution,
            components: change.start_component..change.end_component,
            layers: 0..change.end_layer,
        }
    }

    /// Returns the main header's QCC override for one component.
    fn main_quantization(&self, component: u16) -> Result<Option<Quantization<'a>>, Jpeg2000Error> {
        let components = self.main.size().component_count();
        let mut markers = self.main.markers();
        while let Some(segment) = markers.next_segment()? {
            if segment.marker() != Marker::Qcc {
                continue;
            }
            let found = ComponentQuantization::parse(segment, components)?;
            if found.component == component {
                return Ok(Some(found.quantization));
            }
        }
        Ok(None)
    }

    /// Returns the first RGN shift naming a component in a marker sequence.
    fn find_region(
        mut markers: MarkerReader<'a>,
        component: u16,
        components: u16,
    ) -> Result<Option<u8>, Jpeg2000Error> {
        while let Some(segment) = markers.next_segment()? {
            if segment.marker() != Marker::Rgn {
                continue;
            }
            let region = RegionOfInterest::parse(segment, components)?;
            if region.component == component {
                return Ok(Some(region.shift));
            }
        }
        Ok(None)
    }

    /// Returns a reader over one tile-part header's marker segments.
    fn markers(part: &TilePart<'a>) -> MarkerReader<'a> {
        MarkerReader::new_at(part.header(), part.offset())
    }
}
