//! Prepares recorded pattern cells for repetition by rendering backends.
//!
//! A cell's bounds describe its content; its repeat periods describe the spacing
//! between copies. Cells can leave gaps or overlap. Preparation finds the copies
//! intersecting the period at the origin, before mapping to logical device coordinates.

use std::{iter, sync::Arc};

use pdf_graphics::{
    rect::Rect,
    size::Size,
    transform::{Transform, TransformError},
    viewport::CanvasViewport,
};
use thiserror::Error;

use crate::recording_canvas::{MAX_NESTING, RecordingCanvas};

/// Maximum number of candidate cells examined, including discarded boundary cells.
const MAX_CELL_CANDIDATES: f32 = 4_096.0;

/// Maximum number of recorded commands replayed to populate one repeat period.
///
/// Backends replay the cell recording once per intersecting cell, and a pattern
/// used inside that recording is replayed again for each of those copies, so the
/// work multiplies with every nesting level.
const MAX_REPLAY_COST: usize = 1 << 20;

/// Largest cell index magnitude below which every integer index is exact in `f32`.
const MAX_EXACT_INDEX: f32 = 16_777_216.0;

/// Failure to prepare a tiling pattern's geometry for replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TilingShaderError {
    /// Cell bounds are invalid, or a repeat step is zero or nonfinite.
    #[error("invalid tiling pattern geometry")]
    InvalidGeometry,
    /// The pattern transform or its inverse cannot be represented.
    #[error("invalid tiling pattern transform: {0}")]
    InvalidTransform(#[from] TransformError),
    /// Candidate enumeration or cell replay exceeds the work limit, or a cell index
    /// or offset exceeds numeric representation limits.
    #[error("tiling pattern replay limit exceeded")]
    ReplayLimitExceeded,
    /// The cell recording nests more masks or patterns than backends replay.
    #[error("tiling pattern nesting exceeds {MAX_NESTING} levels")]
    NestingLimit,
    /// One repeat period does not map to a representable pixel raster.
    #[error("tiling pattern period cannot be rasterized")]
    RasterLimit,
}

/// Pixel dimensions and mappings for rasterizing one repeat period.
///
/// The period is sampled at the backing-pixel density of the target so repeated
/// tiles stay sharp under zoom, rotation, and shear.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileRaster {
    /// Backing pixel dimensions of the period raster.
    pub size: Size<u32>,
    /// Maps pattern-space cell coordinates into the period raster's pixels.
    pub cell_to_pixel: Transform,
    /// Maps pattern space into backing pixels of the target being painted.
    pub pattern_to_backing: Transform,
}

/// A recorded pattern cell and the geometry needed to populate one repeat period.
///
/// Construction validates the geometry and fixes the replay order. Backends replay
/// the cell at each supplied translation into a surface covering the repeat period,
/// then repeat that surface using the pattern transform. Clones share both the
/// recording and the prepared translations.
#[derive(Clone)]
pub struct TilingShader {
    /// Recorded cell content in pattern space.
    recording: Arc<RecordingCanvas>,
    /// Mapping from pattern space to logical device space, before backend replay scaling.
    pattern_transform: Transform,
    /// Cell bounds and positive repeat periods in pattern-space units.
    lattice: Lattice,
    /// Translations of intersecting cells, ordered by ascending row and then column.
    cell_transforms: Arc<[Transform]>,
}

impl TilingShader {
    /// Prepares a recording with the given pattern-space bounds and signed steps.
    ///
    /// Negative steps become positive repeat periods. The recording is shared
    /// without copying it.
    ///
    /// # Errors
    ///
    /// Returns [`TilingShaderError::InvalidGeometry`] for invalid bounds or steps,
    /// [`TilingShaderError::NestingLimit`] if the recording nests too deeply,
    /// [`TilingShaderError::InvalidTransform`] if `pattern_transform` is not invertible,
    /// or [`TilingShaderError::ReplayLimitExceeded`] if enumeration requires more than
    /// 4,096 candidates, replaying the cells exceeds the replay work limit, or a cell
    /// index or offset cannot be represented faithfully.
    pub fn new(
        recording: Arc<RecordingCanvas>,
        pattern_transform: Transform,
        cell_bounds: Rect,
        step: [f32; 2],
    ) -> Result<Self, TilingShaderError> {
        validate_geometry(&cell_bounds, step)?;
        if recording.nesting() >= MAX_NESTING {
            return Err(TilingShaderError::NestingLimit);
        }
        pattern_transform.try_inverse()?;

        let lattice = Lattice {
            cell_bounds,
            step: step.map(f32::abs),
        };
        let [x_step, y_step] = lattice.step;
        let cell_transforms = lattice
            .cells_meeting(&Rect {
                left: 0.0,
                top: 0.0,
                right: x_step,
                bottom: y_step,
            })?
            .into();

        let shader = Self {
            recording,
            pattern_transform,
            lattice,
            cell_transforms,
        };
        if shader.replay_cost() > MAX_REPLAY_COST {
            return Err(TilingShaderError::ReplayLimitExceeded);
        }
        Ok(shader)
    }

    /// Returns the recorded cell content to replay at each cell translation.
    pub fn recording(&self) -> &RecordingCanvas {
        &self.recording
    }

    /// Returns the number of recorded commands replayed to populate one repeat period.
    pub fn replay_cost(&self) -> usize {
        self.recording
            .replay_cost()
            .saturating_mul(self.cell_transforms.len())
    }

    /// Returns the pattern-to-logical-device mapping, composed with backend replay mappings.
    pub fn pattern_transform(&self) -> &Transform {
        &self.pattern_transform
    }

    /// Returns the positive horizontal and vertical repeat periods in pattern space.
    pub fn repeat_step(&self) -> [f32; 2] {
        self.lattice.step
    }

    /// Plans a period raster for a target whose logical device maps to backing pixels
    /// through `device_to_backing`.
    ///
    /// Backends replay each cell transform composed after `cell_to_pixel`, then repeat
    /// the raster under `pattern_to_backing` composed with the inverse of `cell_to_pixel`.
    pub fn raster_plan(
        &self,
        device_to_backing: &Transform,
    ) -> Result<TileRaster, TilingShaderError> {
        let pattern_to_backing = device_to_backing.post_concatenated(&self.pattern_transform);
        pattern_to_backing.try_inverse()?;
        let [step_x, step_y] = self.lattice.step;
        let period = Size::new(step_x, step_y);
        let size = pattern_to_backing
            .map_edge_lengths(period)
            .to_pixels()
            .ok_or(TilingShaderError::RasterLimit)?;
        let cell = CanvasViewport::new(period, size).map_err(|_| TilingShaderError::RasterLimit)?;
        Ok(TileRaster {
            size,
            cell_to_pixel: *cell.device_to_backing(),
            pattern_to_backing,
        })
    }

    /// Returns pattern-space translations in ascending row-major replay order.
    ///
    /// Backends must retain this order because overlapping cells may composite
    /// differently when reordered. Cells only touching the period's edge are omitted.
    pub fn cell_transforms(&self) -> &[Transform] {
        &self.cell_transforms
    }

    /// Returns pattern-space translations of every lattice cell meeting `visible`, in
    /// ascending row-major replay order.
    ///
    /// Backends that skip the period raster replay the cell recording once per
    /// translation, directly onto a target whose visible area maps to `visible` in
    /// pattern space. The order matches [`Self::cell_transforms`], so overlapping cells
    /// composite as they do in a period raster.
    ///
    /// # Errors
    ///
    /// Returns [`TilingShaderError::ReplayLimitExceeded`] if enumeration requires more
    /// than 4,096 candidates, replaying the cells exceeds the replay work limit, or a
    /// cell index or offset cannot be represented faithfully.
    pub fn lattice_transforms(&self, visible: &Rect) -> Result<Vec<Transform>, TilingShaderError> {
        let transforms = self.lattice.cells_meeting(visible)?;
        if self
            .recording
            .replay_cost()
            .saturating_mul(transforms.len())
            > MAX_REPLAY_COST
        {
            return Err(TilingShaderError::ReplayLimitExceeded);
        }
        Ok(transforms)
    }
}

/// Validated cell bounds repeated at positive periods along both pattern-space axes.
#[derive(Clone, Copy)]
struct Lattice {
    /// Pattern-space bounds of the cell at the lattice origin.
    cell_bounds: Rect,
    /// Positive horizontal and vertical repeat periods.
    step: [f32; 2],
}

impl Lattice {
    /// Returns translations of the cells whose interiors meet `window`, ordered by
    /// ascending row and then column.
    ///
    /// Row-major traversal defines the compositing order for overlapping cells. Strict
    /// comparisons exclude cells that only touch an edge of the window.
    fn cells_meeting(&self, window: &Rect) -> Result<Vec<Transform>, TilingShaderError> {
        let bounds = self.cell_bounds;
        let [x_step, y_step] = self.step;
        let x_axis = RepeatAxis::new(
            bounds.left,
            bounds.right,
            x_step,
            [window.left, window.right],
        )?;
        let y_axis = RepeatAxis::new(
            bounds.top,
            bounds.bottom,
            y_step,
            [window.top, window.bottom],
        )?;
        // Limit all candidates, not just cells surviving the intersection test, so
        // sparse patterns cannot force unbounded work for a small output.
        if x_axis.candidate_count() * y_axis.candidate_count() > MAX_CELL_CANDIDATES {
            return Err(TilingShaderError::ReplayLimitExceeded);
        }
        Ok(y_axis
            .offsets()
            .flat_map(|y| x_axis.offsets().map(move |x| (x, y)))
            .filter(|&(x, y)| {
                bounds.right + x > window.left
                    && bounds.bottom + y > window.top
                    && bounds.left + x < window.right
                    && bounds.top + y < window.bottom
            })
            .map(|(x, y)| Transform::from_translate(x, y))
            .collect())
    }
}

/// Validates cell bounds and signed repeat steps before transform or range preparation.
pub(crate) fn validate_geometry(bounds: &Rect, step: [f32; 2]) -> Result<(), TilingShaderError> {
    if !bounds.is_valid() || step.iter().any(|value| !value.is_finite() || *value == 0.0) {
        return Err(TilingShaderError::InvalidGeometry);
    }
    Ok(())
}

/// Candidate cell indices along one axis of a repeat period.
#[derive(Clone, Copy)]
struct RepeatAxis {
    /// Positive, finite spacing between cell origins.
    period: f32,
    /// First candidate index, including the lower padding cell.
    first: f32,
    /// Last candidate index, including the upper padding cell.
    last: f32,
}

impl RepeatAxis {
    /// Bounds candidates for a validated cell interval and positive finite period
    /// whose translates may meet the window `[start, end]`.
    fn new(
        lower: f32,
        upper: f32,
        period: f32,
        [start, end]: [f32; 2],
    ) -> Result<Self, TilingShaderError> {
        // A translated interval intersects [start, end] when its upper edge exceeds
        // start and its lower edge precedes end. Keep one extra candidate beyond
        // each rounded bound; the intersection test removes boundary-only cells.
        let first = ((start - upper) / period).floor() - 1.0;
        let last = ((end - lower) / period).ceil() + 1.0;
        // Indices beyond exact integer range would collapse distinct cells onto one
        // origin. Offsets grow monotonically, so finite endpoints bound every offset.
        let representable =
            |index: f32| index.abs() <= MAX_EXACT_INDEX && (index * period).is_finite();
        if !(representable(first) && representable(last)) {
            return Err(TilingShaderError::ReplayLimitExceeded);
        }
        Ok(Self {
            period,
            first,
            last,
        })
    }

    /// Counts the inclusive candidate range.
    fn candidate_count(&self) -> f32 {
        self.last - self.first + 1.0
    }

    /// Yields the pattern-space offset of every candidate cell in ascending order.
    fn offsets(self) -> impl Iterator<Item = f32> {
        iter::successors(Some(self.first), |index| Some(index + 1.0))
            .take_while(move |index| *index <= self.last)
            .map(move |index| index * self.period)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiling_shader_normalizes_steps_and_prepares_overlap() {
        let image = Arc::new(RecordingCanvas::new(6.0, 2.0));
        let shader = TilingShader::new(
            image,
            Transform::identity(),
            Rect::new(6.0, 2.0),
            [-4.0, 4.0],
        )
        .unwrap();

        assert_eq!(shader.repeat_step(), [4.0, 4.0]);
        assert_eq!(*shader.pattern_transform(), Transform::identity());
        assert_eq!(shader.cell_transforms().len(), 2);
    }

    #[test]
    fn tiling_shader_replays_only_visible_cells_of_large_periods() {
        let image = Arc::new(RecordingCanvas::new(3950.0, 350.0));
        let shader = TilingShader::new(
            image,
            Transform::identity(),
            Rect::new(3950.0, 350.0),
            [90_000.0, 90_000.0],
        )
        .unwrap();

        assert_eq!(
            shader
                .lattice_transforms(&Rect::new(4000.0, 400.0))
                .unwrap(),
            [Transform::identity()]
        );
    }

    #[test]
    fn tiling_shader_rejects_invalid_spacing() {
        let image = Arc::new(RecordingCanvas::new(2.0, 2.0));
        assert!(
            TilingShader::new(
                Arc::clone(&image),
                Transform::identity(),
                Rect::new(2.0, 2.0),
                [0.0, 2.0],
            )
            .is_err()
        );
        assert!(
            TilingShader::new(
                image,
                Transform::from_scale(0.0, 1.0),
                Rect::new(2.0, 2.0),
                [2.0, 2.0],
            )
            .is_err()
        );
    }

    #[test]
    fn tiling_shader_rejects_unrepresentable_cell_indexes() {
        let image = Arc::new(RecordingCanvas::new(1.0e23, 2.0));
        let bounds = Rect {
            left: 1.0e30,
            top: 0.0,
            right: 1.000_000_1e30,
            bottom: 2.0,
        };

        assert!(TilingShader::new(image, Transform::identity(), bounds, [1.0e20, 2.0]).is_err());
    }
}
