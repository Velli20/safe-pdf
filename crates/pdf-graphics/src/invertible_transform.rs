//! An affine mapping paired with its validated inverse.

use crate::{
    point::Point,
    transform::{Transform, TransformError},
};

/// A finite transform kept together with its inverse, for spaces mapped in both directions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InvertibleTransform {
    forward: Transform,
    inverse: Transform,
}

impl InvertibleTransform {
    /// Inverts `forward`, rejecting a nonfinite or singular mapping.
    pub fn new(forward: Transform) -> Result<Self, TransformError> {
        Ok(Self {
            inverse: forward.try_inverse()?,
            forward,
        })
    }

    /// Returns the mapping this was created from.
    pub fn forward(&self) -> &Transform {
        &self.forward
    }

    /// Returns the inverse of [`Self::forward`].
    pub fn inverse(&self) -> &Transform {
        &self.inverse
    }

    /// Maps a point back through the inverse, rejecting nonfinite input or output.
    pub fn unmap_point(&self, point: Point) -> Result<Point, TransformError> {
        self.inverse.try_map_point(point)
    }

    /// Maps a displacement back through the inverse, ignoring its translation and
    /// rejecting nonfinite input or output.
    pub fn unmap_delta(&self, delta: Point) -> Result<Point, TransformError> {
        self.inverse.linear().try_map_point(delta)
    }
}
