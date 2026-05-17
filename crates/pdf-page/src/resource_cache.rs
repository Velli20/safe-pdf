use std::collections::{HashMap, HashSet};

pub use crate::lazy_cache_value::LazyCacheValue;
use crate::{resource::Resource, resources::Resources};

/// A trait for managing cached PDF resources by object number.
///
/// [`ResourceCache`] provides an interface for storing and retrieving PDF
/// resources (such as fonts, images, etc.) associated with their object numbers.
/// This allows efficient reuse and lookup of resources during PDF page processing.
/// Implementors of this trait can define custom caching strategies for resource
/// management.
pub trait ResourceCache {
    /// Retrieves a reference to a `Resource` associated with the given object number,
    /// if it exists.
    ///
    /// # Parameters
    ///
    /// - `obj_num`: The object number used as the key to look up the resource.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the `Resource` if found, or `None` if
    /// not present.
    fn get(&self, obj_num: &usize) -> Option<&Resource>;

    /// Inserts a `Resource` into the cache, associating it with the given object number.
    ///
    /// # Parameters
    ///
    /// - `obj_num`: The object number to associate with the resource.
    /// - `resource`: The `Resource` to insert into the cache.
    fn insert(&mut self, obj_num: usize, resource: Resource);

    /// Retrieves a reference to a parsed `/Resources` dictionary associated with the object number.
    fn get_resources(&self, _obj_num: &usize) -> Option<&Resources> {
        None
    }

    /// Inserts a parsed `/Resources` dictionary into the cache.
    fn insert_resources(&mut self, _obj_num: usize, _resources: Resources) {}

    /// Removes a cached resource for the given object number, if present.
    fn remove(&mut self, _obj_num: &usize) -> Option<Resource> {
        None
    }

    /// Removes a cached `/Resources` dictionary for the given object number, if present.
    fn remove_resources(&mut self, _obj_num: &usize) -> Option<Resources> {
        None
    }

    /// Marks the object as currently being parsed.
    ///
    /// Returns `false` when the object is already in progress, indicating a cycle.
    fn begin_read(&mut self, _obj_num: usize) -> bool {
        true
    }

    /// Clears the in-progress marker for the object.
    fn end_read(&mut self, _obj_num: usize) {}

    /// Returns whether the object is currently being parsed.
    fn is_being_read(&self, _obj_num: &usize) -> bool {
        false
    }
}

/// Default resource cache used during page/resource parsing.
#[derive(Default)]
pub struct DefaultResourceCache {
    resources: HashMap<usize, Resource>,
    resource_dictionaries: HashMap<usize, Resources>,
    in_progress: HashSet<usize>,
}

impl ResourceCache for DefaultResourceCache {
    fn get(&self, obj_num: &usize) -> Option<&Resource> {
        self.resources.get(obj_num)
    }

    fn insert(&mut self, obj_num: usize, resource: Resource) {
        self.resources.insert(obj_num, resource);
    }

    fn get_resources(&self, obj_num: &usize) -> Option<&Resources> {
        self.resource_dictionaries.get(obj_num)
    }

    fn insert_resources(&mut self, obj_num: usize, resources: Resources) {
        self.resource_dictionaries.insert(obj_num, resources);
    }

    fn remove(&mut self, obj_num: &usize) -> Option<Resource> {
        self.resources.remove(obj_num)
    }

    fn remove_resources(&mut self, obj_num: &usize) -> Option<Resources> {
        self.resource_dictionaries.remove(obj_num)
    }

    fn begin_read(&mut self, obj_num: usize) -> bool {
        self.in_progress.insert(obj_num)
    }

    fn end_read(&mut self, obj_num: usize) {
        self.in_progress.remove(&obj_num);
    }

    fn is_being_read(&self, obj_num: &usize) -> bool {
        self.in_progress.contains(obj_num)
    }
}

/// Reads a cacheable value while publishing a lazy placeholder in the cache first.
///
/// This is useful for resource kinds such as fonts or `/Resources` dictionaries where recursive lookups
/// should keep the entry alive and resolve it later instead of dropping it
/// when the same object number is encountered again during parsing.
pub fn read_resource_lazy<T, E, F>(
    cache: &mut dyn ResourceCache,
    obj_num: Option<usize>,
    read: F,
) -> Result<T, E>
where
    T: LazyCacheValue,
    F: FnOnce(&mut dyn ResourceCache) -> Result<T, E>,
{
    let Some(obj_num) = obj_num else {
        return read(cache);
    };

    if let Some(cached) = T::get_cached(cache, &obj_num) {
        return Ok(cached.clone());
    }

    let (placeholder, reference) = T::cyclic_reference(obj_num);
    T::insert_cached(cache, obj_num, placeholder);

    match read(cache) {
        Ok(value) => {
            T::resolve(&reference, value.clone());
            T::insert_cached(cache, obj_num, value.clone());
            Ok(value)
        }
        Err(err) => {
            let _ = T::remove_cached(cache, &obj_num);
            Err(err)
        }
    }
}

impl ResourceCache for HashMap<usize, Resource> {
    fn get(&self, obj_num: &usize) -> Option<&Resource> {
        HashMap::get(self, obj_num)
    }

    fn insert(&mut self, obj_num: usize, resource: Resource) {
        HashMap::insert(self, obj_num, resource);
    }

    fn remove(&mut self, obj_num: &usize) -> Option<Resource> {
        HashMap::remove(self, obj_num)
    }
}
