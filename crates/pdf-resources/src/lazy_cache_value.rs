//! Trait implementations for cache entries that support lazy placeholder resolution.

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
pub trait LazyCacheValue: Clone {
    /// The reference handle stored inside the placeholder value.
    type Reference;

    /// Returns a cached value if one already exists for `obj_num`.
    fn get_cached<'a>(cache: &'a dyn ResourceCache, obj_num: &usize) -> Option<&'a Self>;

    /// Inserts `value` into the cache under `obj_num`.
    fn insert_cached(cache: &mut dyn ResourceCache, obj_num: usize, value: Self);

    /// Removes the cached value stored under `obj_num`, if any.
    fn remove_cached(cache: &mut dyn ResourceCache, obj_num: &usize) -> Option<Self>;

    /// Creates a placeholder/reference pair for `object_number`.
    ///
    /// The placeholder is inserted into the cache immediately, while the
    /// returned reference is later resolved with the fully parsed value.
    fn cyclic_reference(object_number: usize) -> (Self, Self::Reference);

    /// Resolves `reference` so it points at the fully parsed `value`.
    fn resolve(reference: &Self::Reference, value: Self);
}

impl LazyCacheValue for Resource {
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

    fn resolve(reference: &Self::Reference, value: Self) {
        reference.resolve(value);
    }
}

impl LazyCacheValue for Resources {
    type Reference = ResourcesReference;

    fn get_cached<'a>(cache: &'a dyn ResourceCache, obj_num: &usize) -> Option<&'a Self> {
        cache.get_resources(obj_num)
    }

    fn insert_cached(cache: &mut dyn ResourceCache, obj_num: usize, value: Self) {
        cache.insert_resources(obj_num, value);
    }

    fn remove_cached(cache: &mut dyn ResourceCache, obj_num: &usize) -> Option<Self> {
        cache.remove_resources(obj_num)
    }

    fn cyclic_reference(object_number: usize) -> (Self, Self::Reference) {
        Self::cyclic_reference(object_number)
    }

    fn resolve(reference: &Self::Reference, value: Self) {
        reference.resolve(value);
    }
}
