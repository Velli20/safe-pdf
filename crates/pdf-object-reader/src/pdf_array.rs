//! Immutable PDF arrays.

use crate::object_variant::ObjectVariant;
use std::slice;
use std::sync::Arc;

/// Stores an immutable ordered sequence of PDF objects.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfArray(Arc<[ObjectVariant]>);

impl PdfArray {
    /// Creates an array from owned PDF object handles.
    pub fn new(objects: impl Into<Vec<ObjectVariant>>) -> Self {
        Self(Arc::from(objects.into()))
    }

    /// Returns the number of values in the array.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the array contains no values.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the value at `index`, if it exists.
    pub fn get(&self, index: usize) -> Option<&ObjectVariant> {
        self.0.get(index)
    }

    /// Iterates over the values in source order.
    pub fn iter(&self) -> slice::Iter<'_, ObjectVariant> {
        self.0.iter()
    }
}
