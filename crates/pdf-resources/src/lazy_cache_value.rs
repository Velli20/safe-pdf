//! Trait implementations for cache entries that support lazy placeholder resolution.

use std::rc::Rc;

use crate::{
    resource::{Resource, ResourceReference},
    resource_cache::ResourceCache,
    resources::Resources,
    resources_reference::ResourcesReference,
};

/// A cache value that can publish a placeholder before parsing finishes.
///
/// Implementations describe how the value is stored in a [`ResourceCache`] and
/// how to create a placeholder/reference pair so recursive lookups can observe
/// the entry while the final value is still being constructed.
pub trait LazyCacheValue: Sized {
    /// The shareable representation stored in and returned from the cache.
    type Shared: Clone;

    /// The reference handle stored inside the placeholder value.
    type Reference;

    /// Returns a cached value if one already exists for `obj_num`.
    fn get_cached<'a>(cache: &'a dyn ResourceCache, obj_num: &usize) -> Option<&'a Self::Shared>;

    /// Inserts `value` into the cache under `obj_num`.
    fn insert_cached(cache: &mut dyn ResourceCache, obj_num: usize, value: Self::Shared);

    /// Removes the cached value stored under `obj_num`, if any.
    fn remove_cached(cache: &mut dyn ResourceCache, obj_num: &usize) -> Option<Self::Shared>;

    /// Creates a placeholder/reference pair for `object_number`.
    ///
    /// The placeholder is inserted into the cache immediately, while the
    /// returned reference is later resolved with the fully parsed value.
    fn cyclic_reference(object_number: usize) -> (Self::Shared, Self::Reference);

    /// Converts a freshly parsed value into its shareable representation.
    fn into_shared(value: Self) -> Self::Shared;

    /// Resolves `reference` so it points at the fully parsed `value`.
    fn resolve(reference: &Self::Reference, value: &Self::Shared);
}

impl LazyCacheValue for Resource {
    type Shared = Self;
    type Reference = ResourceReference;

    fn get_cached<'a>(cache: &'a dyn ResourceCache, obj_num: &usize) -> Option<&'a Self> {
        cache.get(obj_num)
    }

    fn insert_cached(cache: &mut dyn ResourceCache, obj_num: usize, value: Self) {
        cache.insert(obj_num, value);
    }

    fn remove_cached(cache: &mut dyn ResourceCache, obj_num: &usize) -> Option<Self> {
        cache.remove(obj_num)
    }

    fn cyclic_reference(object_number: usize) -> (Self, Self::Reference) {
        Self::cyclic_reference(object_number)
    }

    fn into_shared(value: Self) -> Self::Shared {
        value
    }

    fn resolve(reference: &Self::Reference, value: &Self::Shared) {
        reference.resolve(value.clone());
    }
}

impl LazyCacheValue for Resources {
    type Shared = Rc<Self>;
    type Reference = ResourcesReference;

    fn get_cached<'a>(cache: &'a dyn ResourceCache, obj_num: &usize) -> Option<&'a Self::Shared> {
        cache.get_resources(obj_num)
    }

    fn insert_cached(cache: &mut dyn ResourceCache, obj_num: usize, value: Self::Shared) {
        cache.insert_resources(obj_num, value);
    }

    fn remove_cached(cache: &mut dyn ResourceCache, obj_num: &usize) -> Option<Self::Shared> {
        cache.remove_resources(obj_num)
    }

    fn cyclic_reference(object_number: usize) -> (Self::Shared, Self::Reference) {
        let (placeholder, reference) = Self::cyclic_reference(object_number);
        (Rc::new(placeholder), reference)
    }

    fn into_shared(value: Self) -> Self::Shared {
        Rc::new(value)
    }

    fn resolve(reference: &Self::Reference, value: &Self::Shared) {
        reference.resolve(Rc::clone(value));
    }
}
