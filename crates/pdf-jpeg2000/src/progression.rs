//! Packet ordering within a tile.
//!
//! Annex B.12 defines five progression orders over the layer, resolution,
//! component, and precinct axes, and Annex A.6.6 lets a POC segment replace
//! the COD order with a sequence of sub-ranges, each with its own order. The
//! result is one flat packet sequence per tile, which is what the tile-part
//! bodies carry.
//!
//! The sequence is materialised into a list rather than produced by a state
//! machine: the orders differ only in their loop nesting, so a list keeps each
//! order a handful of lines and lets the tile decoder index packets directly.
//! The list is charged against the caller's working-memory bound.

use core::ops::Range;

use pdf_graphics::{Size, point::Point};

use crate::{
    Jpeg2000Error,
    coding::ProgressionOrder,
    position_scan::{PositionScan, PrecinctProjection},
    region::Region,
    workspace::Workspace,
};

/// Identifies one packet of a tile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PacketLocator {
    /// Quality layer the packet belongs to.
    pub(crate) layer: u16,
    /// Resolution level the packet belongs to.
    pub(crate) resolution: u8,
    /// Component the packet belongs to.
    pub(crate) component: u16,
    /// Raster index of the precinct within its resolution level.
    pub(crate) precinct: u32,
}

/// Precinct layout of one resolution level of one tile-component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolutionProgression {
    /// Extent of the level on the resolution grid.
    pub(crate) region: Region,
    /// Precinct exponents on the resolution grid.
    pub(crate) exponents: Size<u8>,
    /// Precinct columns in the level.
    pub(crate) columns: u32,
    /// Precinct rows in the level.
    pub(crate) rows: u32,
}

impl ResolutionProgression {
    /// Returns the number of precincts in the level.
    pub(crate) fn count(self) -> u32 {
        self.columns.saturating_mul(self.rows)
    }
}

/// Resolution levels of one tile-component, in increasing order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComponentProgression {
    /// Subsampling factors declared by SIZ for this component.
    pub(crate) subsampling: Size<u8>,
    /// Decomposition levels applied to this component.
    pub(crate) levels: u8,
    /// One entry per resolution level from zero to `levels`.
    pub(crate) resolutions: Vec<ResolutionProgression>,
}

impl ComponentProgression {
    /// Returns one resolution level, or `None` above the component's count.
    pub(crate) fn resolution(&self, index: u8) -> Option<&ResolutionProgression> {
        self.resolutions.get(usize::from(index))
    }

    /// Returns the number of resolution levels the component carries.
    pub(crate) fn resolution_count(&self) -> u8 {
        u8::try_from(self.resolutions.len()).unwrap_or(u8::MAX)
    }
}

/// One progression order applied over a range of the four packet axes.
///
/// A codestream without POC has a single step spanning every axis; a POC
/// segment contributes one step per record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProgressionStep {
    /// Order in which the step's packets are emitted.
    pub(crate) order: ProgressionOrder,
    /// Resolution levels covered by the step.
    pub(crate) resolutions: Range<u8>,
    /// Components covered by the step.
    pub(crate) components: Range<u16>,
    /// Quality layers covered by the step.
    pub(crate) layers: Range<u16>,
}

/// Builds the packet sequence of one tile.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ProgressionPlan<'a> {
    tile: Region,
    components: &'a [ComponentProgression],
}

impl<'a> ProgressionPlan<'a> {
    /// Prepares to order the packets of one tile.
    pub(crate) fn new(tile: Region, components: &'a [ComponentProgression]) -> Self {
        Self { tile, components }
    }

    /// Appends every packet of the given steps, in codestream order.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` when the sequence passes the caller's
    /// working-memory bound, and `Overflow` on unrepresentable geometry.
    pub(crate) fn build(
        &self,
        steps: &[ProgressionStep],
        into: &mut Vec<PacketLocator>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        for step in steps {
            match step.order {
                ProgressionOrder::LayerResolutionComponentPosition => {
                    self.layer_first(step, into, workspace)?;
                }
                ProgressionOrder::ResolutionLayerComponentPosition => {
                    self.resolution_first(step, into, workspace)?;
                }
                ProgressionOrder::ResolutionPositionComponentLayer => {
                    self.resolution_position(step, into, workspace)?;
                }
                ProgressionOrder::PositionComponentResolutionLayer => {
                    self.position_first(step, into, workspace)?;
                }
                ProgressionOrder::ComponentPositionResolutionLayer => {
                    self.component_first(step, into, workspace)?;
                }
            }
        }
        Ok(())
    }

    /// Emits packets in layer, resolution, component, precinct order.
    fn layer_first(
        &self,
        step: &ProgressionStep,
        into: &mut Vec<PacketLocator>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        for layer in step.layers.clone() {
            for resolution in step.resolutions.clone() {
                for component in step.components.clone() {
                    self.emit_precincts(layer, resolution, component, into, workspace)?;
                }
            }
        }
        Ok(())
    }

    /// Emits packets in resolution, layer, component, precinct order.
    fn resolution_first(
        &self,
        step: &ProgressionStep,
        into: &mut Vec<PacketLocator>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        for resolution in step.resolutions.clone() {
            for layer in step.layers.clone() {
                for component in step.components.clone() {
                    self.emit_precincts(layer, resolution, component, into, workspace)?;
                }
            }
        }
        Ok(())
    }

    /// Emits packets in resolution, position, component, layer order.
    fn resolution_position(
        &self,
        step: &ProgressionStep,
        into: &mut Vec<PacketLocator>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        for resolution in step.resolutions.clone() {
            let levels = resolution..resolution.saturating_add(1);
            let stride = self.stride(&step.components, &levels)?;
            for position in self.positions(stride) {
                for component in step.components.clone() {
                    self.emit_position(step, component, resolution, position, into, workspace)?;
                }
            }
        }
        Ok(())
    }

    /// Emits packets in position, component, resolution, layer order.
    fn position_first(
        &self,
        step: &ProgressionStep,
        into: &mut Vec<PacketLocator>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        let stride = self.stride(&step.components, &step.resolutions)?;
        for position in self.positions(stride) {
            for component in step.components.clone() {
                for resolution in step.resolutions.clone() {
                    self.emit_position(step, component, resolution, position, into, workspace)?;
                }
            }
        }
        Ok(())
    }

    /// Emits packets in component, position, resolution, layer order.
    fn component_first(
        &self,
        step: &ProgressionStep,
        into: &mut Vec<PacketLocator>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        for component in step.components.clone() {
            let scope = component..component.saturating_add(1);
            let stride = self.stride(&scope, &step.resolutions)?;
            for position in self.positions(stride) {
                for resolution in step.resolutions.clone() {
                    self.emit_position(step, component, resolution, position, into, workspace)?;
                }
            }
        }
        Ok(())
    }

    /// Emits every precinct of one layer, resolution, and component.
    fn emit_precincts(
        &self,
        layer: u16,
        resolution: u8,
        component: u16,
        into: &mut Vec<PacketLocator>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        let Some(level) = self.level(component, resolution) else {
            return Ok(());
        };
        for precinct in 0..level.count() {
            workspace.push(
                into,
                PacketLocator {
                    layer,
                    resolution,
                    component,
                    precinct,
                },
            )?;
        }
        Ok(())
    }

    /// Emits every layer of the precinct starting at a reference-grid point.
    fn emit_position(
        &self,
        step: &ProgressionStep,
        component: u16,
        resolution: u8,
        position: Point<u32>,
        into: &mut Vec<PacketLocator>,
        workspace: &mut Workspace,
    ) -> Result<(), Jpeg2000Error> {
        let Some(entry) = self.component(component) else {
            return Ok(());
        };
        let Some(projection) = PrecinctProjection::new(entry, resolution)? else {
            return Ok(());
        };
        let Some(precinct) = projection.precinct_at(self.tile, position)? else {
            return Ok(());
        };
        for layer in step.layers.clone() {
            workspace.push(
                into,
                PacketLocator {
                    layer,
                    resolution,
                    component,
                    precinct,
                },
            )?;
        }
        Ok(())
    }

    /// Returns the reference-grid positions visited at a given stride.
    fn positions(&self, stride: Size<u64>) -> impl Iterator<Item = Point<u32>> {
        let origin = self.tile.origin();
        let end = self.tile.end();
        let rows = PositionScan::new(origin.y, end.y, stride.height);
        let columns = PositionScan::new(origin.x, end.x, stride.width);
        rows.positions()
            .flat_map(move |y| columns.positions().map(move |x| Point { x, y }))
    }

    /// Returns the smallest precinct stride over a scope of the packet axes.
    fn stride(
        &self,
        components: &Range<u16>,
        resolutions: &Range<u8>,
    ) -> Result<Size<u64>, Jpeg2000Error> {
        let mut stride = Size {
            width: u64::MAX,
            height: u64::MAX,
        };
        for component in components.clone() {
            let Some(entry) = self.component(component) else {
                continue;
            };
            for resolution in resolutions.clone() {
                let Some(projection) = PrecinctProjection::new(entry, resolution)? else {
                    continue;
                };
                if !projection.populated() {
                    continue;
                }
                let candidate = projection.stride();
                stride = Size {
                    width: stride.width.min(candidate.width),
                    height: stride.height.min(candidate.height),
                };
            }
        }
        Ok(Size {
            width: stride.width.min(u64::from(u32::MAX)).max(1),
            height: stride.height.min(u64::from(u32::MAX)).max(1),
        })
    }

    /// Returns one component's resolution table.
    fn component(&self, index: u16) -> Option<&'a ComponentProgression> {
        self.components.get(usize::from(index))
    }

    /// Returns one resolution level of one component.
    fn level(&self, component: u16, resolution: u8) -> Option<&'a ResolutionProgression> {
        self.component(component)?.resolution(resolution)
    }
}
