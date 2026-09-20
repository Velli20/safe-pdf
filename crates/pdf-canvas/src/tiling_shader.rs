//! Prepares recorded pattern cells for repetition by rendering backends.
//!
//! A cell's bounds describe its content; its repeat periods describe the spacing
//! between copies. Cells can leave gaps or overlap. Preparation finds the copies
//! intersecting the period at the origin, before mapping to logical device coordinates.

use std::sync::Arc;

use num_traits::ToPrimitive;
use pdf_graphics::{
    rect::Rect,
    transform::{Transform, TransformError},
};
use thiserror::Error;

use crate::{
    recording_canvas::{MAX_NESTING, RecordingCanvas},
    viewport::pixel_extent,
};

/// Maximum number of candidate cells examined, including discarded boundary cells.
const MAX_CELL_CANDIDATES: usize = 4_096;

/// Failure to prepare a tiling pattern's geometry for replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TilingShaderError {
    /// Cell bounds are invalid, or a repeat step is zero or nonfinite.
    #[error("invalid tiling pattern geometry")]
    InvalidGeometry,
    /// The pattern transform or its inverse cannot be represented.
    #[error("invalid tiling pattern transform: {0}")]
    InvalidTransform(#[from] TransformError),
    /// Candidate enumeration exceeds the work limit or numeric representation limits.
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
    pub size: [u32; 2],
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
    /// Original cell bounds in pattern space, including any offset from the origin.
    cell_bounds: Rect,
    /// Positive horizontal and vertical repeat periods in pattern-space units.
    repeat_step: [f32; 2],
    /// Translations of intersecting cells, ordered by ascending row and then column.
    cell_transforms: Arc<[Transform]>,
}

impl TilingShader {
    /// Prepares a recording with the given pattern-space bounds and signed steps.
    ///
    /// Negative steps become positive repeat periods. An absent pattern-to-device
    /// transform defaults to identity. The recording is shared without copying it.
    ///
    /// # Errors
    ///
    /// Returns [`TilingShaderError::InvalidGeometry`] for invalid bounds or steps,
    /// [`TilingShaderError::InvalidTransform`] if inversion fails,
    /// [`TilingShaderError::ReplayLimitExceeded`] if enumeration requires more than
    /// 4,096 candidates or an index or offset cannot be represented faithfully, or
    /// [`TilingShaderError::NestingLimit`] if the recording nests too deeply.
    pub fn new(
        recording: Arc<RecordingCanvas>,
        transform: Option<Transform>,
        cell_bounds: Rect,
        step: [f32; 2],
    ) -> Result<Self, TilingShaderError> {
        validate_geometry(&cell_bounds, step)?;
        if recording.nesting() >= MAX_NESTING {
            return Err(TilingShaderError::NestingLimit);
        }
        let pattern_transform = transform.unwrap_or_else(Transform::identity);
        pattern_transform.try_inverse()?;

        let [x_step, y_step] = step.map(f32::abs);
        let x_axis = RepeatAxis::new(cell_bounds.left, cell_bounds.right, x_step)?;
        let y_axis = RepeatAxis::new(cell_bounds.top, cell_bounds.bottom, y_step)?;
        let capacity = candidate_capacity(&x_axis, &y_axis)?;
        let cell_transforms = collect_cell_transforms(&cell_bounds, &x_axis, &y_axis, capacity)?;

        Ok(Self {
            recording,
            pattern_transform,
            cell_bounds,
            repeat_step: [x_step, y_step],
            cell_transforms: cell_transforms.into(),
        })
    }

    /// Returns the recorded cell content to replay at each cell translation.
    pub fn recording(&self) -> &RecordingCanvas {
        &self.recording
    }

    /// Returns the pattern-to-logical-device mapping, composed with backend replay mappings.
    pub fn pattern_transform(&self) -> &Transform {
        &self.pattern_transform
    }

    /// Returns the original cell bounds in pattern space.
    pub fn cell_bounds(&self) -> Rect {
        self.cell_bounds
    }

    /// Returns the positive horizontal and vertical repeat periods in pattern space.
    pub fn repeat_step(&self) -> [f32; 2] {
        self.repeat_step
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
        let [step_x, step_y] = self.repeat_step;
        let [density_x, density_y] = pattern_to_backing.axis_scales();
        let size = [
            pixel_extent(step_x * density_x).ok_or(TilingShaderError::RasterLimit)?,
            pixel_extent(step_y * density_y).ok_or(TilingShaderError::RasterLimit)?,
        ];
        let [width, height] = size;
        let cell_to_pixel = Transform::from_scale(
            width.to_f32().ok_or(TilingShaderError::RasterLimit)? / step_x,
            height.to_f32().ok_or(TilingShaderError::RasterLimit)? / step_y,
        );
        Ok(TileRaster {
            size,
            cell_to_pixel,
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
}

/// Validates cell bounds and signed repeat steps before transform or range preparation.
fn validate_geometry(bounds: &Rect, step: [f32; 2]) -> Result<(), TilingShaderError> {
    if !bounds.is_valid() || step.iter().any(|value| !value.is_finite() || *value == 0.0) {
        return Err(TilingShaderError::InvalidGeometry);
    }
    Ok(())
}

/// Candidate cell indices along one axis of a repeat period.
struct RepeatAxis {
    /// Positive, finite spacing between cell origins.
    period: f32,
    /// First candidate index, including the lower padding cell.
    start: i64,
    /// Last candidate index, including the upper padding cell.
    end: i64,
}

impl RepeatAxis {
    /// Bounds candidates for a validated cell interval and positive finite period.
    fn new(lower: f32, upper: f32, period: f32) -> Result<Self, TilingShaderError> {
        // A translated interval intersects [0, period] when its upper edge exceeds
        // zero and its lower edge precedes period. Keep one extra candidate beyond
        // each rounded bound; the intersection predicate removes boundary-only cells.
        let start = (((-upper) / period).floor() - 1.0)
            .to_i64()
            .ok_or(TilingShaderError::ReplayLimitExceeded)?;
        let end = (((period - lower) / period).ceil() + 1.0)
            .to_i64()
            .ok_or(TilingShaderError::ReplayLimitExceeded)?;
        Ok(Self { period, start, end })
    }

    /// Counts the inclusive candidate range without overflowing signed index arithmetic.
    fn candidate_count(&self) -> Result<i64, TilingShaderError> {
        self.end
            .checked_sub(self.start)
            .and_then(|count| count.checked_add(1))
            .ok_or(TilingShaderError::ReplayLimitExceeded)
    }

    /// Converts an exactly representable cell index into a finite pattern-space offset.
    fn offset(&self, index: i64) -> Result<f32, TilingShaderError> {
        // A lossy index conversion could collapse distinct cells onto the same origin.
        let coordinate = index
            .to_f32()
            .filter(|coordinate| coordinate.to_i64() == Some(index))
            .ok_or(TilingShaderError::ReplayLimitExceeded)?;
        let offset = coordinate * self.period;
        if !offset.is_finite() {
            return Err(TilingShaderError::ReplayLimitExceeded);
        }
        Ok(offset)
    }
}

/// Bounds enumeration work and allocation before visiting any candidate cells.
fn candidate_capacity(
    x_axis: &RepeatAxis,
    y_axis: &RepeatAxis,
) -> Result<usize, TilingShaderError> {
    let x_count = x_axis.candidate_count()?;
    let y_count = y_axis.candidate_count()?;
    // Limit all candidates, not just cells surviving the intersection test, so
    // sparse patterns cannot force unbounded work for a small output allocation.
    x_count
        .checked_mul(y_count)
        .and_then(|count| usize::try_from(count).ok())
        .filter(|count| *count <= MAX_CELL_CANDIDATES)
        .ok_or(TilingShaderError::ReplayLimitExceeded)
}

/// Collects intersecting translations using the previously checked candidate capacity.
fn collect_cell_transforms(
    bounds: &Rect,
    x_axis: &RepeatAxis,
    y_axis: &RepeatAxis,
    capacity: usize,
) -> Result<Vec<Transform>, TilingShaderError> {
    let mut transforms = Vec::with_capacity(capacity);
    // Row-major traversal defines the compositing order for overlapping cells.
    // Validate every candidate's offsets before filtering, including padded cells.
    for y in y_axis.start..=y_axis.end {
        let y_offset = y_axis.offset(y)?;
        for x in x_axis.start..=x_axis.end {
            let x_offset = x_axis.offset(x)?;
            if intersects_period(bounds, x_offset, y_offset, x_axis.period, y_axis.period) {
                // At most one translation is emitted per checked candidate.
                transforms.push(Transform::from_translate(x_offset, y_offset));
            }
        }
    }
    Ok(transforms)
}

/// Tests for positive-area overlap between a translated cell and the origin period.
fn intersects_period(bounds: &Rect, x: f32, y: f32, width: f32, height: f32) -> bool {
    // Strict comparisons exclude cells that only touch an edge of the period.
    bounds.right + x > 0.0
        && bounds.bottom + y > 0.0
        && bounds.left + x < width
        && bounds.top + y < height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiling_shader_normalizes_steps_and_prepares_overlap() {
        let image = Arc::new(RecordingCanvas::new(6.0, 2.0));
        let shader = TilingShader::new(image, None, Rect::new(6.0, 2.0), [-4.0, 4.0]).unwrap();

        assert_eq!(shader.repeat_step(), [4.0, 4.0]);
        assert_eq!(*shader.pattern_transform(), Transform::identity());
        assert_eq!(shader.cell_transforms().len(), 2);
    }

    #[test]
    fn tiling_shader_rejects_invalid_spacing() {
        let image = Arc::new(RecordingCanvas::new(2.0, 2.0));
        assert!(
            TilingShader::new(Arc::clone(&image), None, Rect::new(2.0, 2.0), [0.0, 2.0],).is_err()
        );
        assert!(
            TilingShader::new(
                image,
                Some(Transform::from_scale(0.0, 1.0)),
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

        assert!(TilingShader::new(image, None, bounds, [1.0e20, 2.0]).is_err());
    }
}
