//! Immutable PDF arrays.

use crate::object_variant::ObjectVariant;
use std::ops::Deref;
use std::slice;
use std::sync::Arc;

/// Stores an immutable ordered sequence of PDF objects.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfArray(Arc<Vec<ObjectVariant>>);

impl Deref for PdfArray {
    type Target = [ObjectVariant];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}

impl PdfArray {
    /// Creates an array from owned PDF object handles.
    pub fn new(objects: impl Into<Vec<ObjectVariant>>) -> Self {
        Self(Arc::new(objects.into()))
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

    pub fn as_slice(&self) -> &[ObjectVariant] {
        self.0.as_slice()
    }

    pub fn first(&self) -> Option<&ObjectVariant> {
        self.0.first()
    }
}

impl From<Vec<ObjectVariant>> for PdfArray {
    fn from(v: Vec<ObjectVariant>) -> Self {
        Self::new(v)
    }
}

impl FromIterator<ObjectVariant> for PdfArray {
    fn from_iter<T: IntoIterator<Item = ObjectVariant>>(i: T) -> Self {
        Self::from(i.into_iter().collect::<Vec<_>>())
    }
}

impl<'a> IntoIterator for &'a PdfArray {
    type Item = &'a ObjectVariant;
    type IntoIter = slice::Iter<'a, ObjectVariant>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl IntoIterator for PdfArray {
    type Item = ObjectVariant;
    type IntoIter = std::vec::IntoIter<ObjectVariant>;

    fn into_iter(self) -> Self::IntoIter {
        match Arc::try_unwrap(self.0) {
            Ok(values) => values.into_iter(),
            Err(values) => values.as_ref().clone().into_iter(),
        }
    }
}
